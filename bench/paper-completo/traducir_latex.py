"""Variante: <m id="N">LaTeX</m>. Comprueba que el LaTeX vuelve intacto y en orden."""
import sys, re, json, html, pathlib
sys.path.insert(0, str(pathlib.Path(__file__).parent))
from traducir import ask
from bs4 import BeautifulSoup

rows = json.loads(pathlib.Path(sys.argv[1]).read_text())
papers = {}
for r in rows:
    papers.setdefault(r['paper'], []).append(r)
for pid, rs in papers.items():
    soup = BeautifulSoup(pathlib.Path(pathlib.Path(sys.argv[1]).parent, pid + '.html').read_text(), 'lxml')
    cands = [p for p in soup.select('p.ltx_p') if len(p.find_all('math')) >= 2 and 300 <= len(p.get_text()) <= 1400 and not p.find_parent(class_='ltx_bibliography')]
    for n, p in enumerate(cands[:: max(1, len(cands) // 3)][:3]):
        lat = [m.get('alttext', '') for m in p.find_all('math')]
        s = rs[n]['original']
        for i, l in enumerate(lat):
            s = s.replace(f'<m id="{i}"></m>', f'<m id="{i}">{html.escape(l, quote=False)}</m>')
        out, secs, usage = ask(s)
        got = [html.unescape(x) for x in re.findall(r'<m id="\d+">(.*?)</m>', out, re.S)]
        print(json.dumps(dict(paper=pid, n=n, segundos=round(secs, 1), tokens_out=usage['completion_tokens'],
              latex_intacto=got == lat, asteriscos='*' in out and '*' not in s,
              extracto=re.search(r'\(1\).{0,160}', out).group(0) if '(1)' in out else ''), ensure_ascii=False), flush=True)
