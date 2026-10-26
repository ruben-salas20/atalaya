#!/usr/bin/env python3
"""Mide cuánto tarda en verse en español el principio de un texto largo (#32).

Uso: python3 bench/por-partes/medir.py SALIDA.json

Traduce, con el código del servicio (ejemplo `traducir_parrafos`), las tres
noticias más largas del conjunto de prueba y el paper 2610.31646 de arXiv,
párrafo a párrafo y en orden, como hace hoy la cola de la GPU. Por cada texto
apunta cuándo estuvo listo cada párrafo: el primero, el 25 %, el 50 % y el
total. Necesita `bench/traduccion/textos-completos.json` (ver corpus.py) y
llama-swap. Usa la GPU: solo una medida a la vez.
"""
import json
import pathlib
import re
import subprocess
import sys
import urllib.request

HERE = pathlib.Path(__file__).resolve().parent
REPO = HERE.parents[1]
sys.path.insert(0, str(REPO / 'bench' / 'traduccion'))
import corpus  # noqa: E402

PAPER = '2610.31646v1'


def paper_body():
    """El cuerpo del paper sin bibliografía: lo que se traduciría."""
    html = urllib.request.urlopen('https://arxiv.org/html/%s/' % PAPER, timeout=60).read().decode()
    start = html.find('<article')
    end = html.find('<section class="ltx_bibliography"')
    return html[start:end if end > 0 else html.find('</article>')]


def main():
    out = pathlib.Path(sys.argv[1])
    noticias = [t for t in corpus.load(corpus.default_path(), {'noticia'}) if t['formato'] == 'html']
    largas = sorted(noticias, key=lambda t: len(t['texto']), reverse=True)[:3]
    textos = [{'id': t['id'], 'html': t['texto']} for t in largas]
    textos.append({'id': 'paper-' + PAPER, 'html': paper_body()})
    cmd = ['cargo', 'run', '-q', '-p', 'atalaya-server', '--example', 'traducir_parrafos']
    proc = subprocess.run(cmd, cwd=REPO, input=json.dumps(textos).encode(), stdout=subprocess.PIPE, check=True)
    filas = []
    for r in json.loads(proc.stdout):
        s = r['segundos']
        n = len(s)
        fila = {
            'id': r['id'], 'parrafos': n, 'en_ingles': r['en_ingles'],
            'primero_s': round(s[0], 1) if s else None,
            'cuarto_s': round(s[max(0, n // 4 - 1)], 1) if s else None,
            'mitad_s': round(s[max(0, n // 2 - 1)], 1) if s else None,
            'total_s': round(s[-1], 1) if s else None,
            'segundos': s,
        }
        filas.append(fila)
        print('%-22s %3d párrafos · primero %5.1f s · 25 %% %6.1f s · 50 %% %6.1f s · total %6.1f s'
              % (fila['id'], n, fila['primero_s'], fila['cuarto_s'], fila['mitad_s'], fila['total_s']))
    out.write_text(json.dumps(filas, ensure_ascii=False, indent=1))


if __name__ == '__main__':
    main()
