pub const DEFAULT_BATCH_SIZE: usize = 50;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MutationBatch {
    mutations: Vec<String>,
    batch_size: usize,
}

impl MutationBatch {
    pub fn new(batch_size: usize) -> Self {
        Self {
            mutations: Vec::new(),
            batch_size: normalize_batch_size(batch_size),
        }
    }

    pub fn push(&mut self, mutation_body: String) -> bool {
        let mutation_body = normalize_mutation_body(&mutation_body);
        if mutation_body.is_empty() {
            return self.is_full();
        }

        self.mutations.push(mutation_body);
        self.is_full()
    }

    pub fn build(&self) -> Option<String> {
        if self.mutations.is_empty() {
            return None;
        }

        let aliased = self
            .mutations
            .iter()
            .enumerate()
            .map(|(index, mutation)| format!("m{index}: {mutation}"))
            .collect::<Vec<_>>()
            .join("\n");

        Some(format!("mutation {{\n{aliased}\n}}"))
    }

    pub fn take(&mut self) -> Option<String> {
        let query = self.build()?;
        self.mutations.clear();
        Some(query)
    }

    pub fn is_empty(&self) -> bool {
        self.mutations.is_empty()
    }

    pub fn len(&self) -> usize {
        self.mutations.len()
    }

    pub fn batch_size(&self) -> usize {
        self.batch_size
    }

    fn is_full(&self) -> bool {
        self.mutations.len() >= self.batch_size
    }
}

fn normalize_batch_size(batch_size: usize) -> usize {
    if batch_size == 0 {
        DEFAULT_BATCH_SIZE
    } else {
        batch_size
    }
}

fn normalize_mutation_body(mutation: &str) -> String {
    let trimmed = mutation.trim();
    if !trimmed.starts_with("mutation") {
        return trimmed.to_string();
    }

    let Some(open) = trimmed.find('{') else {
        return trimmed.to_string();
    };
    let Some(close) = trimmed.rfind('}') else {
        return trimmed.to_string();
    };
    if open >= close {
        return trimmed.to_string();
    }

    trimmed[open + 1..close].trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_aliases_mutations_and_strips_wrappers() {
        let mut batch = MutationBatch::new(2);
        batch.push(
            "mutation {\n  create_CodingMessage(input: { message_id: \"a\" }) { _docID }\n}"
                .to_string(),
        );
        batch.push("create_CodingAction(input: { action_type: \"Read\" }) { _docID }".to_string());

        let query = batch.build().unwrap();

        assert!(query.contains("m0: create_CodingMessage(input: { message_id: \"a\" }) { _docID }"));
        assert!(
            query.contains("m1: create_CodingAction(input: { action_type: \"Read\" }) { _docID }")
        );
        assert!(query.starts_with("mutation {"));
    }

    #[test]
    fn take_clears_batch() {
        let mut batch = MutationBatch::new(1);
        batch.push("create_CodingMessage(input: { message_id: \"a\" }) { _docID }".to_string());

        assert!(batch.take().is_some());
        assert!(batch.is_empty());
    }
}
