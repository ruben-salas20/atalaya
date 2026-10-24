"""Traduce párrafos reales de papers con Gemma y comprueba fórmulas y citas.

Estrategia «crudo»: el párrafo tal cual, con su MathML.
Estrategia «marcas»: cada <math> se cambia por <m id="N"></m>; se quitan los
atributos salvo href. Tras traducir, cada marca vuelve a su fórmula.
"""
import sys, re, json, time, pathlib, urllib.request
from bs4 import BeautifulSoup, NavigableString

URL = 'https://llama.rubensalas.dev/v1/chat/completions'
PROMPT = ("Translate the following HTML fragment from English into Latin American Spanish. "
          "Keep every HTML tag and attribute exactly as it is, including links (<a>) and emphasis (<em>, <i>, <b>, <strong>), "
          "and translate only the visible text. Reply with the translated fragment only, without notes, explanations or code fences.\n\n")

def ask(text):
    payload = {'model': 'gemma-4-e4b-rapido', 'temperature': 0.2, 'max_tokens': 6000,
               'messages': [{'role': 'user', 'content': PROMPT + text}]}
    req = urllib.request.Request(URL, json.dumps(payload).encode(), {'Content-Type': 'application/json'})
    t0 = time.perf_counter()
    r = json.load(urllib.request.urlopen(req, timeout=900))
    return r['choices'][0]['message']['content'].strip(), time.perf_counter() - t0, r['usage']

def simplify(p):
    p = BeautifulSoup(str(p), 'lxml').find('p')
    for i, m in enumerate(p.find_all('math')):
        m.replace_with(BeautifulSoup(f'<m id="{i}"></m>', 'lxml').find('m'))
    for t in p.find_all(True):
        if t.name in ('a', 'em', 'b', 'i', 'strong', 'm'):
            t.attrs = {k: v for k, v in t.attrs.items() if k in ('href', 'id')}
        else:
            t.unwrap()
    return ''.join(str(c) for c in p.contents).strip()

def main():
    results = []
    for path in sys.argv[1:]:
        soup = BeautifulSoup(pathlib.Path(path).read_text(), 'lxml')
        cands = [p for p in soup.select('p.ltx_p')
                 if len(p.find_all('math')) >= 2 and 300 <= len(p.get_text()) <= 1400 and not p.find_parent(class_='ltx_bibliography')]
        picks = cands[:: max(1, len(cands) // 3)][:3]
        for n, p in enumerate(picks):
            simple = simplify(p)
            out, secs, usage = ask(simple)
            ids_o = re.findall(r'<m id="(\d+)"', simple)
            ids_t = re.findall(r'<m id="(\d+)"', out)
            links_o = re.findall(r'href="([^"]+)"', simple); links_t = re.findall(r'href="([^"]+)"', out)
            row = dict(paper=pathlib.Path(path).stem, estrategia='marcas', n=n, segundos=round(secs, 1),
                       tokens_in=usage['prompt_tokens'], tokens_out=usage['completion_tokens'],
                       marcas_ok=ids_o == ids_t, marcas=f'{len(ids_t)}/{len(ids_o)}', enlaces_ok=links_o == links_t,
                       original=simple, traduccion=out)
            if n == 0:
                raw = ''.join(str(c) for c in p.contents).strip()
                rout, rsecs, rusage = ask(raw)
                row['crudo'] = dict(segundos=round(rsecs, 1), tokens_in=rusage['prompt_tokens'], tokens_out=rusage['completion_tokens'],
                                    maths=f"{rout.count('<math')}/{raw.count('<math')}", alttext_igual=re.findall(r'alttext="([^"]*)"', raw) == re.findall(r'alttext="([^"]*)"', rout))
            print(json.dumps({k: v for k, v in row.items() if k not in ('original', 'traduccion')}, ensure_ascii=False), flush=True)
            results.append(row)
    pathlib.Path(sys.argv[1]).parent.joinpath('traducciones.json').write_text(json.dumps(results, ensure_ascii=False, indent=1))


if __name__ == '__main__':
    main()
