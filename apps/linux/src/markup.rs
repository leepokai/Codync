//! Markdown → Pango markup (the subset agents actually send).

use gtk::glib::markup_escape_text;

fn inline(line: &str) -> String {
    let escaped = markup_escape_text(line).to_string();
    let mut out = String::new();
    let mut code = false;
    for (i, part) in escaped.split('`').enumerate() {
        if i > 0 {
            code = !code;
        }
        if code {
            out.push_str(&format!("<tt>{part}</tt>"));
        } else {
            out.push_str(&emphasis(part));
        }
    }
    out
}

fn emphasis(s: &str) -> String {
    let mut out = String::new();
    for (i, part) in s.split("**").enumerate() {
        if i % 2 == 1 {
            out.push_str(&format!("<b>{part}</b>"))
        } else {
            out.push_str(&links(part))
        }
    }
    out
}

fn links(s: &str) -> String {
    // [text](http…) → <a>
    let mut out = String::new();
    let mut rest = s;
    while let Some(open) = rest.find('[') {
        let Some(close) = rest[open..].find("](").map(|i| open + i) else {
            break;
        };
        let Some(end) = rest[close..].find(')').map(|i| close + i) else {
            break;
        };
        let url = &rest[close + 2..end];
        if !url.starts_with("http") {
            break;
        }
        out.push_str(&rest[..open]);
        out.push_str(&format!("<a href=\"{url}\">{}</a>", &rest[open + 1..close]));
        rest = &rest[end + 1..];
    }
    out.push_str(rest);
    out
}

pub fn to_pango(src: &str) -> String {
    let mut out = Vec::new();
    let mut in_code = false;
    for line in src.lines() {
        let t = line.trim_start();
        if t.starts_with("```") {
            in_code = !in_code;
            continue;
        }
        if in_code {
            out.push(format!("<tt>{}</tt>", markup_escape_text(line)));
        } else if let Some(h) = t
            .strip_prefix("### ")
            .or_else(|| t.strip_prefix("## "))
            .or_else(|| t.strip_prefix("# "))
        {
            out.push(format!("<b>{}</b>", inline(h)));
        } else if let Some(item) = t.strip_prefix("- ").or_else(|| t.strip_prefix("* ")) {
            out.push(format!("  •  {}", inline(item)));
        } else {
            out.push(inline(line));
        }
    }
    out.join("\n")
}

#[cfg(test)]
mod tests {
    #[test]
    fn converts_common_markdown() {
        assert_eq!(super::to_pango("**hi** `a<b`"), "<b>hi</b> <tt>a&lt;b</tt>");
        assert_eq!(
            super::to_pango("- x\n```\nfn()\n```"),
            "  •  x\n<tt>fn()</tt>"
        );
        assert_eq!(
            super::to_pango("[doc](https://a.b)"),
            "<a href=\"https://a.b\">doc</a>"
        );
    }
}
