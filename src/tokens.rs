use nadi_core::parser::tokenizer::Token;

pub(crate) trait TokenMarkup {
    fn markup(&self) -> String;
}

impl<'a> TokenMarkup for Token<'a> {
    fn markup(&self) -> String {
        format!(
            "<span foreground=\"{}\">{}</span>",
            self.ty.syntax_color(),
            self.content
                .replace("&", "&amp;")
                .replace("<", "&lt;")
                .replace(">", "&gt;")
        )
    }
}
