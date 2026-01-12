use std::collections::HashMap;

pub trait CredentialKey {
    fn credential_key(&self) -> &str;
}

pub fn rebuild_index<T: CredentialKey>(index: &mut HashMap<String, usize>, items: &[T]) {
    index.clear();
    for (idx, item) in items.iter().enumerate() {
        let key = item.credential_key().trim();
        if key.is_empty() {
            continue;
        }
        index.insert(key.to_string(), idx);
    }
}

pub fn find_index<T: CredentialKey>(
    index: &HashMap<String, usize>,
    items: &[T],
    key: &str,
) -> Option<usize> {
    let idx = *index.get(key)?;
    let item = items.get(idx)?;
    if item.credential_key() == key {
        Some(idx)
    } else {
        None
    }
}

pub fn find_or_rebuild<T: CredentialKey>(
    index: &mut HashMap<String, usize>,
    items: &[T],
    key: &str,
) -> Option<usize> {
    if let Some(idx) = find_index(index, items, key) {
        return Some(idx);
    }
    rebuild_index(index, items);
    find_index(index, items, key)
}

pub fn ensure_index<T: CredentialKey>(index: &mut HashMap<String, usize>, items: &[T]) {
    if index.is_empty() && !items.is_empty() {
        rebuild_index(index, items);
    }
}

pub fn update_index_on_change(
    index: &mut HashMap<String, usize>,
    old_key: &str,
    new_key: &str,
    idx: usize,
) {
    if old_key != new_key {
        index.remove(old_key);
    }
    if new_key.trim().is_empty() {
        return;
    }
    index.insert(new_key.to_string(), idx);
}
