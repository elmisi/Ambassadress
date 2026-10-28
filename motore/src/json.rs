//! Un piccolo scrittore JSON, per non portare serde nel motore. Scrive solo
//! quello che serve alla vista: stringhe, numeri, booleani, null, oggetti e liste,
//! con le virgole al posto giusto.

/// Scrive una stringa JSON tra virgolette.
///
/// Oltre agli escape obbligatori (`"`, `\` e i caratteri di controllo, in `\u00XX`
/// quando non hanno una forma breve) scrive ` ` e ` `, che in JavaScript
/// sono a capo, e `<` prima di `/` e `!` come `<`: così il testo si può mettere
/// anche dentro un tag `<script>` senza chiuderlo per sbaglio.
pub fn stringa(out: &mut String, s: &str) {
    out.reserve(s.len() + 2);
    out.push('"');
    let mut caratteri = s.chars().peekable();
    while let Some(c) = caratteri.next() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\u{8}' => out.push_str("\\b"),
            '\u{c}' => out.push_str("\\f"),
            '\u{2028}' => out.push_str("\\u2028"),
            '\u{2029}' => out.push_str("\\u2029"),
            '<' if matches!(caratteri.peek(), Some('/') | Some('!')) => out.push_str("\\u003c"),
            c if (c as u32) < 0x20 || c == '\u{7f}' => {
                out.push_str(&format!("\\u{:04x}", c as u32));
            }
            c => out.push(c),
        }
    }
    out.push('"');
}

/// Un oggetto o una lista in costruzione: mette le virgole fra gli elementi.
pub struct Scrittore<'a> {
    out: &'a mut String,
    primo: bool,
}

impl<'a> Scrittore<'a> {
    pub fn oggetto(out: &'a mut String) -> Self {
        out.push('{');
        Scrittore { out, primo: true }
    }

    pub fn lista(out: &'a mut String) -> Self {
        out.push('[');
        Scrittore { out, primo: true }
    }

    fn separa(&mut self) {
        if !self.primo {
            self.out.push(',');
        }
        self.primo = false;
    }

    /// La chiave di un campo; il valore va scritto subito dopo con `valore`.
    pub fn chiave(&mut self, k: &str) -> &mut String {
        self.separa();
        stringa(self.out, k);
        self.out.push(':');
        self.out
    }

    /// Un elemento della lista, da scrivere nella stringa restituita.
    pub fn elemento(&mut self) -> &mut String {
        self.separa();
        self.out
    }

    pub fn testo(&mut self, k: &str, v: &str) {
        stringa(self.chiave(k), v);
    }

    pub fn testo_o_null(&mut self, k: &str, v: Option<&str>) {
        match v {
            Some(v) => self.testo(k, v),
            None => self.chiave(k).push_str("null"),
        }
    }

    pub fn numero(&mut self, k: &str, v: i64) {
        let s = v.to_string();
        self.chiave(k).push_str(&s);
    }

    pub fn booleano(&mut self, k: &str, v: bool) {
        self.chiave(k).push_str(if v { "true" } else { "false" });
    }

    pub fn chiudi_oggetto(self) {
        self.out.push('}');
    }

    pub fn chiudi_lista(self) {
        self.out.push(']');
    }
}

#[cfg(test)]
mod prove {
    use super::*;

    #[test]
    fn escape() {
        let mut s = String::new();
        stringa(&mut s, "a\"b\\c\nd\u{1}\u{1f}é😊\u{2028}</script><!--<b>");
        assert_eq!(
            s,
            "\"a\\\"b\\\\c\\nd\\u0001\\u001fé😊\\u2028\\u003c/script>\\u003c!--<b>\""
        );
    }

    #[test]
    fn oggetti_e_liste() {
        let mut s = String::new();
        let mut o = Scrittore::oggetto(&mut s);
        o.testo("a", "x");
        o.numero("n", -3);
        o.booleano("b", true);
        o.testo_o_null("z", None);
        {
            let mut l = Scrittore::lista(o.chiave("l"));
            stringa(l.elemento(), "uno");
            stringa(l.elemento(), "due");
            l.chiudi_lista();
        }
        o.chiudi_oggetto();
        assert_eq!(s, r#"{"a":"x","n":-3,"b":true,"z":null,"l":["uno","due"]}"#);
    }
}
