#!/usr/bin/env python3
"""Generate a professional PDF from the Thymus technical document."""

import markdown
import weasyprint
from pathlib import Path

SCRIPT_DIR = Path(__file__).parent
MD_FILE = SCRIPT_DIR / "Thymus_Document_Technique.md"
PDF_FILE = SCRIPT_DIR / "Thymus_Document_Technique.pdf"

CSS = """
@page {
    size: A4;
    margin: 2.5cm 2cm 2.5cm 2cm;
    @top-center {
        content: "THYMUS — Document de Conception Technique";
        font-size: 8pt;
        color: #666;
        font-family: 'Noto Sans', 'DejaVu Sans', sans-serif;
    }
    @bottom-center {
        content: counter(page) " / " counter(pages);
        font-size: 8pt;
        color: #666;
        font-family: 'Noto Sans', 'DejaVu Sans', sans-serif;
    }
}

@page :first {
    @top-center { content: none; }
    @bottom-center { content: none; }
}

body {
    font-family: 'Noto Sans', 'DejaVu Sans', sans-serif;
    font-size: 10.5pt;
    line-height: 1.6;
    color: #1a1a1a;
    text-align: justify;
}

h1 {
    font-size: 20pt;
    color: #0d1b2a;
    border-bottom: 3px solid #1b4965;
    padding-bottom: 8px;
    margin-top: 40px;
    page-break-before: always;
}

h1:first-of-type {
    font-size: 28pt;
    text-align: center;
    border-bottom: 4px solid #1b4965;
    page-break-before: avoid;
    margin-top: 80px;
}

h2 {
    font-size: 15pt;
    color: #1b4965;
    margin-top: 28px;
    border-bottom: 1px solid #bee9e8;
    padding-bottom: 4px;
}

h3 {
    font-size: 12pt;
    color: #415a77;
    margin-top: 20px;
}

h4 {
    font-size: 11pt;
    color: #62929e;
    margin-top: 16px;
}

p {
    margin-bottom: 8px;
}

code {
    font-family: 'Noto Mono', 'DejaVu Sans Mono', monospace;
    background-color: #f0f4f8;
    padding: 1px 4px;
    border-radius: 3px;
    font-size: 9pt;
    color: #1b4965;
}

pre {
    background-color: #0d1b2a;
    color: #e0e1dd;
    padding: 14px 18px;
    border-radius: 6px;
    font-family: 'Noto Mono', 'DejaVu Sans Mono', monospace;
    font-size: 8.5pt;
    line-height: 1.5;
    overflow-x: auto;
    page-break-inside: avoid;
    margin: 12px 0;
    border-left: 4px solid #1b4965;
}

pre code {
    background-color: transparent;
    color: #e0e1dd;
    padding: 0;
    font-size: 8.5pt;
}

table {
    width: 100%;
    border-collapse: collapse;
    margin: 16px 0;
    font-size: 9.5pt;
    page-break-inside: avoid;
}

th {
    background-color: #1b4965;
    color: white;
    padding: 10px 12px;
    text-align: left;
    font-weight: 600;
}

td {
    padding: 8px 12px;
    border-bottom: 1px solid #dee2e6;
}

tr:nth-child(even) td {
    background-color: #f8f9fa;
}

blockquote {
    border-left: 4px solid #1b4965;
    margin: 16px 0;
    padding: 12px 20px;
    background-color: #e8f4f8;
    color: #1a1a1a;
    font-style: italic;
    page-break-inside: avoid;
}

blockquote strong {
    color: #1b4965;
}

ul, ol {
    margin-bottom: 10px;
    padding-left: 24px;
}

li {
    margin-bottom: 4px;
}

strong {
    color: #0d1b2a;
}

hr {
    border: none;
    border-top: 2px solid #1b4965;
    margin: 30px 0;
}

/* Cover page styling */
body > h1:first-of-type + h2 {
    text-align: center;
    border: none;
    color: #415a77;
    font-size: 13pt;
    page-break-before: avoid;
}

/* Special styling for the table of contents section */
a {
    color: #1b4965;
    text-decoration: none;
}
"""

def main():
    md_text = MD_FILE.read_text(encoding="utf-8")

    html_body = markdown.markdown(
        md_text,
        extensions=["tables", "fenced_code", "toc", "nl2br"],
    )

    cover = """
    <div style="text-align:center; margin-top:120px; margin-bottom:60px;">
        <div style="font-size:14pt; color:#415a77; letter-spacing:4px; margin-bottom:20px;">CONFIDENTIEL</div>
        <div style="width:120px; height:4px; background:#1b4965; margin:0 auto 40px;"></div>
        <div style="font-size:10pt; color:#666;">
            <p><strong>Version :</strong> 1.0 &nbsp;&nbsp;|&nbsp;&nbsp; <strong>Date :</strong> 2 juin 2026</p>
            <p><strong>Classification :</strong> Confidentiel — Usage interne Thymus</p>
            <p><strong>Destinataires :</strong> Equipe d'ingénierie Thymus</p>
        </div>
    </div>
    <div style="page-break-after: always;"></div>
    """

    full_html = f"""<!DOCTYPE html>
<html lang="fr">
<head>
    <meta charset="utf-8">
    <style>{CSS}</style>
</head>
<body>
{cover}
{html_body}
</body>
</html>"""

    doc = weasyprint.HTML(string=full_html)
    doc.write_pdf(str(PDF_FILE))
    print(f"PDF generated: {PDF_FILE}")
    print(f"Size: {PDF_FILE.stat().st_size / 1024:.0f} KB")

if __name__ == "__main__":
    main()
