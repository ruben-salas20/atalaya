"""Mide un paper en HTML de arXiv: qué hay que traducir y qué hay que pintar."""
import sys, re, json, pathlib
from bs4 import BeautifulSoup

out = {}
for path in sys.argv[1:]:
    soup = BeautifulSoup(pathlib.Path(path).read_text(), 'lxml')
    art = soup.find('article') or soup.body
    bib = art.find(class_='ltx_bibliography')
    bib_chars = len(bib.get_text(' ', strip=True)) if bib else 0
    if bib: bib.decompose()
    maths = art.find_all('math')
    inline = [m for m in maths if m.get('display') != 'block']
    block = [m for m in maths if m.get('display') == 'block']
    latex_chars = sum(len(m.get('alttext', '')) for m in maths)
    mathml_chars = sum(len(str(m)) for m in maths)
    tables = art.select('table.ltx_tabular')
    imgs = art.find_all('img')
    paras = art.select('p.ltx_p')
    for m in art.find_all('math'): m.replace_with(' ')
    for t in art.select('table.ltx_equation, table.ltx_equationgroup'): t.decompose()
    text = re.sub(r'\s+', ' ', art.get_text(' ', strip=True))
    out[pathlib.Path(path).stem] = dict(
        html_bytes=pathlib.Path(path).stat().st_size, paragraphs=len(paras),
        text_chars=len(text), bib_chars=bib_chars,
        math_inline=len(inline), math_block=len(block),
        latex_chars=latex_chars, mathml_chars=mathml_chars,
        data_tables=len(tables), images=len(imgs),
        img_srcs=[i.get('src') for i in imgs][:3],
    )
print(json.dumps(out, indent=1, ensure_ascii=False))
