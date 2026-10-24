use std::collections::HashMap;

pub(super) fn prepare(syllables: &mut [Vec<String>], classes: &HashMap<String, String>) {
    let vowel = |phone: &str| {
        classes
            .get(phone)
            .is_some_and(|class| matches!(class.as_str(), "vowel" | "diphthong"))
    };
    for index in 1..syllables.len() {
        if syllables[index - 1].last().is_some_and(|p| vowel(p))
            && syllables[index].first().is_some_and(|p| vowel(p))
        {

            syllables[index].insert(0, "cl".into());
        }
    }
}
