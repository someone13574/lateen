pub struct Line {
    pub name: String,
    pub value: String,
    parameters: Vec<(String, String)>,
}

impl Line {
    pub fn unfold(text: &str) -> Vec<String> {
        let mut lines: Vec<String> = Vec::new();

        for raw in text.lines() {
            match raw.strip_prefix([' ', '\t']) {
                Some(continuation) => match lines.last_mut() {
                    Some(line) => line.push_str(continuation),
                    None => continue,
                },
                None => lines.push(raw.to_string()),
            }
        }

        lines
    }

    pub fn parse(text: &str) -> Option<Self> {
        let mut quoted = false;
        let (colon, _) = text.char_indices().find(|(_, character)| match character {
            '"' => {
                quoted = !quoted;
                false
            }
            ':' => !quoted,
            _ => false,
        })?;
        let mut fields = text[..colon].split(';');

        Some(Self {
            name: fields.next()?.to_ascii_uppercase(),
            value: text[colon + 1..].to_string(),
            parameters: fields.filter_map(Self::parameter).collect(),
        })
    }

    pub fn parameter_value(&self, name: &str) -> Option<&str> {
        self.parameters
            .iter()
            .find(|(parameter, _)| parameter == name)
            .map(|(_, value)| value.as_str())
    }

    pub fn text(&self) -> String {
        let mut text = String::with_capacity(self.value.len());
        let mut characters = self.value.chars();

        while let Some(character) = characters.next() {
            match character {
                '\\' => match characters.next() {
                    Some('n' | 'N') => text.push(' '),
                    Some(escaped) => text.push(escaped),
                    None => break,
                },
                _ => text.push(character),
            }
        }

        text.trim().to_string()
    }

    fn parameter(field: &str) -> Option<(String, String)> {
        let (name, value) = field.split_once('=')?;

        Some((
            name.to_ascii_uppercase(),
            value.trim_matches('"').to_string(),
        ))
    }
}
