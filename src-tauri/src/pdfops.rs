//! PDF tools built on lopdf: merge, split, compress.

use lopdf::{Dictionary, Document, Object, ObjectId, SaveOptions};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// Page attributes a page can inherit from its ancestors in the page tree.
const INHERITABLE: [&[u8]; 4] = [b"Resources", b"MediaBox", b"CropBox", b"Rotate"];

fn load(path: &Path) -> Result<Document, String> {
    let doc = Document::load(path).map_err(|e| format!("Can't read this PDF: {e}"))?;
    if doc.is_encrypted() {
        return Err("This PDF is password protected".into());
    }
    Ok(doc)
}

/// The page dictionary with inherited attributes copied in, so it can live under a new parent.
fn page_with_inherited(doc: &Document, page_id: ObjectId) -> Result<Dictionary, String> {
    let mut page = doc.get_dictionary(page_id).map_err(|e| e.to_string())?.clone();
    let mut parent = page.get(b"Parent").and_then(Object::as_reference).ok();
    let mut guard = 0;
    while let Some(pid) = parent {
        guard += 1;
        if guard > 64 {
            break;
        }
        let Ok(node) = doc.get_dictionary(pid) else { break };
        for key in INHERITABLE {
            if !page.has(key) {
                if let Ok(v) = node.get(key) {
                    page.set(key.to_vec(), v.clone());
                }
            }
        }
        parent = node.get(b"Parent").and_then(Object::as_reference).ok();
    }
    if !page.has(b"MediaBox") {
        page.set("MediaBox", vec![0.into(), 0.into(), 612.into(), 792.into()]);
    }
    Ok(page)
}

fn save(doc: &mut Document, out: &Path) -> Result<(), String> {
    let file = std::fs::File::create(out).map_err(|e| e.to_string())?;
    let mut w = std::io::BufWriter::new(file);
    let res = doc.save_to(&mut w).map_err(|e| e.to_string());
    drop(w);
    if res.is_err() {
        let _ = std::fs::remove_file(out);
    }
    res
}

/// Combine PDFs in the given order.
pub fn merge(inputs: &[PathBuf], out: &Path) -> Result<(), String> {
    if inputs.len() < 2 {
        return Err("Select at least two PDFs to merge".into());
    }
    let mut merged = Document::with_version("1.5");
    let pages_id = merged.new_object_id();
    let mut kids: Vec<Object> = Vec::new();
    let mut next_id = merged.max_id + 1;

    for path in inputs {
        let mut doc = load(path)?;
        doc.renumber_objects_with(next_id);
        next_id = doc.max_id + 1;

        let page_ids: Vec<ObjectId> = doc.get_pages().into_values().collect();
        let mut pages: BTreeMap<ObjectId, Dictionary> = BTreeMap::new();
        for id in &page_ids {
            let mut page = page_with_inherited(&doc, *id)?;
            page.set("Parent", pages_id);
            pages.insert(*id, page);
        }
        for (id, obj) in doc.objects {
            match obj.type_name().unwrap_or(b"") {
                b"Catalog" | b"Pages" | b"Outlines" | b"Outline" => {}
                b"Page" if pages.contains_key(&id) => {}
                _ => {
                    merged.objects.insert(id, obj);
                }
            }
        }
        for id in page_ids {
            if let Some(page) = pages.remove(&id) {
                merged.objects.insert(id, Object::Dictionary(page));
                kids.push(id.into());
            }
        }
    }

    let count = kids.len() as i64;
    merged.objects.insert(
        pages_id,
        Object::Dictionary(lopdf::dictionary! { "Type" => "Pages", "Kids" => kids, "Count" => count }),
    );
    merged.max_id = next_id;
    let catalog_id = merged.add_object(lopdf::dictionary! { "Type" => "Catalog", "Pages" => pages_id });
    merged.trailer.set("Root", catalog_id);
    merged.prune_objects();
    merged.renumber_objects();
    save(&mut merged, out)
}

/// One PDF per page: `name (page 1).pdf`, `name (page 2).pdf`, ...
pub fn split(input: &Path, out_dir: &Path, stem: &str) -> Result<Vec<PathBuf>, String> {
    let doc = load(input)?;
    let numbers: Vec<u32> = doc.get_pages().keys().copied().collect();
    if numbers.len() < 2 {
        return Err("This PDF has only one page".into());
    }
    let mut outs = Vec::new();
    for &keep in &numbers {
        let mut single = doc.clone();
        let others: Vec<u32> = numbers.iter().copied().filter(|n| *n != keep).collect();
        single.delete_pages(&others);
        single.prune_objects();
        single.renumber_objects();
        let out = crate::actions::unique_path(out_dir, &format!("{stem} (page {keep})"), "pdf");
        if let Err(e) = save(&mut single, &out) {
            for o in &outs {
                let _ = std::fs::remove_file(o);
            }
            return Err(e);
        }
        outs.push(out);
    }
    Ok(outs)
}

/// Re-save with compressed streams and object streams. Returns false (and writes nothing)
/// when the result would not be smaller.
pub fn compress(input: &Path, out: &Path) -> Result<bool, String> {
    let mut doc = load(input)?;
    doc.prune_objects();
    doc.delete_zero_length_streams();
    doc.compress();
    let mut buf = Vec::new();
    let opts = SaveOptions::builder().use_object_streams(true).use_xref_streams(true).compression_level(9).build();
    doc.save_with_options(&mut buf, opts).map_err(|e| e.to_string())?;
    let before = std::fs::metadata(input).map(|m| m.len()).unwrap_or(u64::MAX);
    if (buf.len() as u64) >= before {
        return Ok(false);
    }
    std::fs::write(out, buf).map_err(|e| e.to_string())?;
    Ok(true)
}

#[cfg(test)]
pub fn page_count(input: &Path) -> Result<usize, String> {
    Ok(load(input)?.get_pages().len())
}
