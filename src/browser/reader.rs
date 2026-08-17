use scraper::{ElementRef, Html, Node, Selector};

pub const ULTRA_BANNER_FR: &str =
    "Mode Ultra : JavaScript, images et médias sont coupés. Les applications web peuvent être vides. Ctrl+Shift+E pour changer de mode.";

const DROP_TAGS: &[&str] = &[
    "script", "style", "iframe", "img", "picture", "source", "nav", "header", "footer", "aside",
    "video", "audio", "canvas", "svg", "noscript", "form", "button", "input",
];

const KEEP_TAGS: &[&str] = &[
    "h1", "h2", "h3", "h4", "h5", "h6", "p", "ul", "ol", "li", "blockquote", "pre", "code", "a",
    "em", "strong", "br",
];

const READER_CSS: &str = r#"
html, body { background: #f4f1e8; color: #1a1a1a; }
body { max-width: 40rem; margin: 1.5rem auto; padding: 0 1rem; font: 18px/1.55 Georgia, "Times New Roman", serif; }
.liteweb-ultra-banner { font: 14px/1.4 sans-serif; color: #4a4033; border-bottom: 1px solid #d4cbb8; padding-bottom: 0.75rem; }
a { color: #1a365d; }
* { animation: none !important; transition: none !important; }
"#;

pub fn flatten_html(html: &str, page_url: &str) -> String {
    let document = Html::parse_document(html);
    let title = extract_title(&document, page_url);
    let body = select_root(&document)
        .map(|root| serialize_children(root, page_url))
        .unwrap_or_default();

    format!(
        "<!DOCTYPE html>\n<html lang=\"fr\">\n<head>\n<meta charset=\"utf-8\">\n<title>{title}</title>\n<style>{css}</style>\n</head>\n<body>\n<p class=\"liteweb-ultra-banner\">{banner}</p>\n<article>\n{body}\n</article>\n</body>\n</html>",
        title = escape_text(&title),
        css = READER_CSS,
        banner = escape_text(ULTRA_BANNER_FR),
        body = body,
    )
}

fn extract_title(document: &Html, page_url: &str) -> String {
    if let Some(text) = first_text(document, "title") {
        return text;
    }
    if let Some(text) = first_text(document, "h1") {
        return text;
    }
    page_url.to_string()
}

fn first_text(document: &Html, selector: &str) -> Option<String> {
    let selector = Selector::parse(selector).ok()?;
    let el = document.select(&selector).next()?;
    let text = el.text().collect::<String>();
    let trimmed = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed)
    }
}

fn select_root<'a>(document: &'a Html) -> Option<ElementRef<'a>> {
    for selector in ["article", "main", "body"] {
        let Ok(sel) = Selector::parse(selector) else {
            continue;
        };
        if let Some(el) = document.select(&sel).next() {
            return Some(el);
        }
    }
    None
}

fn serialize_element(el: ElementRef<'_>, page_url: &str) -> String {
    let name = el.value().name();
    if DROP_TAGS.contains(&name) {
        return String::new();
    }
    if name == "br" {
        return "<br>".to_string();
    }
    if KEEP_TAGS.contains(&name) {
        let inner = serialize_children(el, page_url);
        if name == "a" {
            return match resolve_href(el.value().attr("href"), page_url) {
                Some(href) => format!("<a href=\"{}\">{}</a>", escape_text(&href), inner),
                None => inner,
            };
        }
        return format!("<{name}>{inner}</{name}>");
    }
    serialize_children(el, page_url)
}

fn serialize_children(el: ElementRef<'_>, page_url: &str) -> String {
    let mut out = String::new();
    for child in el.children() {
        match child.value() {
            Node::Text(text) => out.push_str(&escape_text(text)),
            Node::Element(_) => {
                if let Some(child_el) = ElementRef::wrap(child) {
                    out.push_str(&serialize_element(child_el, page_url));
                }
            }
            _ => {}
        }
    }
    out
}

fn resolve_href(href: Option<&str>, page_url: &str) -> Option<String> {
    let href = href?.trim();
    if href.is_empty() {
        return None;
    }
    let resolved = url::Url::parse(page_url).ok()?.join(href).ok()?;
    matches!(resolved.scheme(), "http" | "https")
        .then(|| resolved.to_string())
}

fn escape_text(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    for ch in input.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            _ => out.push(ch),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const ARTICLE: &str = r#"<html><head><title>Titre</title>
        <script>alert(1)</script>
        <style>body{background:url(x)}</style></head>
        <body>
          <nav>Menu</nav>
          <article>
            <h1>Titre</h1>
            <p>Premier paragraphe.</p>
            <p>Deuxième avec <a href="/suite">lien</a>.</p>
            <ul><li>Item</li></ul>
          </article>
          <iframe src="https://tracker.example"></iframe>
          <img src="huge.jpg">
        </body></html>"#;

    #[test]
    fn keeps_article_text_and_resolves_links() {
        let out = flatten_html(ARTICLE, "https://example.com/news");
        assert!(out.contains("Premier paragraphe."));
        assert!(out.contains("https://example.com/suite"));
        assert!(out.contains("<h1>"));
        assert!(out.contains(ULTRA_BANNER_FR));
    }

    #[test]
    fn strips_script_style_iframe_img_and_nav() {
        let out = flatten_html(ARTICLE, "https://example.com/news");
        let low = out.to_ascii_lowercase();
        assert!(!low.contains("<script"));
        assert!(!low.contains("alert("));
        assert!(!low.contains("background:url"));
        assert!(!low.contains("<iframe"));
        assert!(!low.contains("<img"));
        assert!(!low.contains("menu"));
    }

    #[test]
    fn empty_spa_still_returns_a_banner_document() {
        let out = flatten_html(
            "<html><body><div id='app'></div></body></html>",
            "https://mail.example",
        );
        assert!(out.contains(ULTRA_BANNER_FR));
        assert!(out.contains("<article"));
    }
}
