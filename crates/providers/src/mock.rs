use crate::{ClassificationProvider, ClassificationRequest, ClassificationResponse, DraftActionItem, ProviderError};
use async_trait::async_trait;

/// Development-only stand-in for a real AI provider. Splits the note into
/// lines, treating each non-indented line as a new draft action item and
/// each tab/space-indented line as a sub-detail of the item above it
/// (appended to its title) — matching the note-taking convention of using
/// indentation to relate a line to the one above. This exists purely so the
/// UI (card list, tags, lifecycle controls) has real data to work against
/// while the Anthropic/OpenAI/Mistral adapters are still stubs — it is never
/// meant to ship as a real classification path.
pub struct MockProvider;

#[async_trait]
impl ClassificationProvider for MockProvider {
    fn name(&self) -> &'static str {
        "mock"
    }

    async fn classify(
        &self,
        request: ClassificationRequest,
    ) -> Result<ClassificationResponse, ProviderError> {
        let mut action_items: Vec<DraftActionItem> = Vec::new();

        for raw_line in request.note_text.lines() {
            if raw_line.trim().is_empty() {
                continue;
            }
            let is_indented = raw_line.starts_with(' ') || raw_line.starts_with('\t');
            let text = raw_line.trim();

            if is_indented {
                if let Some(last) = action_items.last_mut() {
                    last.title = format!("{} — {}", last.title, text);
                    continue;
                }
                // An indented line with nothing above it has no parent to
                // attach to — fall through and treat it as its own item.
            }

            action_items.push(DraftActionItem {
                title: text.to_string(),
                effort: None,
                due_date: None,
                general_tags: vec!["misc".to_string()],
                custom_tags: vec![],
            });
        }

        Ok(ClassificationResponse {
            is_actionable: !action_items.is_empty(),
            action_items,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn splits_note_text_into_one_item_per_top_level_line() {
        let provider = MockProvider;
        let request = ClassificationRequest {
            note_text: "Call Jerry\n\nNeed docs on project\n".to_string(),
            existing_general_tags: vec![],
            existing_custom_tags: vec![],
        };

        let response = provider.classify(request).await.unwrap();
        assert!(response.is_actionable);
        assert_eq!(response.action_items.len(), 2);
        assert_eq!(response.action_items[0].title, "Call Jerry");
        assert_eq!(response.action_items[1].title, "Need docs on project");
    }

    #[tokio::test]
    async fn indented_lines_attach_to_the_item_above_instead_of_becoming_new_items() {
        let provider = MockProvider;
        let request = ClassificationRequest {
            note_text: "Do a meeting with someone\n\tCall jerry\n\tNeed docs on project".to_string(),
            existing_general_tags: vec![],
            existing_custom_tags: vec![],
        };

        let response = provider.classify(request).await.unwrap();
        assert_eq!(response.action_items.len(), 1);
        assert_eq!(
            response.action_items[0].title,
            "Do a meeting with someone — Call jerry — Need docs on project"
        );
    }

    #[tokio::test]
    async fn leading_indented_line_with_no_parent_becomes_its_own_item() {
        let provider = MockProvider;
        let request = ClassificationRequest {
            note_text: "\tOrphan detail line".to_string(),
            existing_general_tags: vec![],
            existing_custom_tags: vec![],
        };

        let response = provider.classify(request).await.unwrap();
        assert_eq!(response.action_items.len(), 1);
        assert_eq!(response.action_items[0].title, "Orphan detail line");
    }

    #[tokio::test]
    async fn blank_note_is_not_actionable() {
        let provider = MockProvider;
        let request = ClassificationRequest {
            note_text: "   \n\n  ".to_string(),
            existing_general_tags: vec![],
            existing_custom_tags: vec![],
        };

        let response = provider.classify(request).await.unwrap();
        assert!(!response.is_actionable);
        assert!(response.action_items.is_empty());
    }
}
