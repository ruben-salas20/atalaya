#!/usr/bin/env python3
"""Cuenta cuántos párrafos de un conjunto fijo de noticias quedan en inglés.

Uso:
    python3 medir-parrafos.py [--url URL] [--modelo MODELO] [--limite N] [--salida ARCHIVO]

Toma las 10 noticias de textos.json (las mismas URLs que medir.py) y las
traduce con el código de atalaya-server del repo: lanza el ejemplo
`traducir_parrafos`, que usa el mismo camino que el servicio. Así se mide
el código que hay en el commit actual.

Para comparar, ejecútalo en el commit de antes y en el de después del cambio,
con el mismo modelo. El resultado guarda el commit y si el repo tenía cambios
sin confirmar.

No cambia nada del repo. Solo usa la biblioteca estándar de Python y cargo.
"""
import argparse
import datetime as dt
import json
import pathlib
import subprocess
import sys

import corpus

HERE = pathlib.Path(__file__).resolve().parent
REPO = HERE.parents[1]
DEFAULT_URL = 'https://llama.rubensalas.dev'
DEFAULT_MODEL = 'gemma-4-e4b-rapido'


def git(*args):
    return subprocess.run(['git', *args], cwd=REPO, capture_output=True, text=True, check=True).stdout.strip()


def translate_articles(articles, url, model):
    entrada = json.dumps(articles, ensure_ascii=False)
    cmd = ['cargo', 'run', '-q', '-p', 'atalaya-server', '--example', 'traducir_parrafos',
           '--', '--url', url, '--model', model]
    # Los avisos por artículo salen por la salida de errores; se muestran tal cual.
    proc = subprocess.run(cmd, cwd=REPO, input=entrada.encode(), stdout=subprocess.PIPE)
    if proc.returncode != 0:
        sys.exit('traducir_parrafos falló (código %d). Revisa la salida de arriba.' % proc.returncode)
    return json.loads(proc.stdout.decode())


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument('--url', default=DEFAULT_URL, help='base de llama-swap (por defecto %(default)s)')
    ap.add_argument('--modelo', default=DEFAULT_MODEL, help='id del modelo en llama-swap (por defecto %(default)s)')
    ap.add_argument('--textos', default=str(corpus.default_path()))
    ap.add_argument('--limite', type=int, help='como mucho N noticias (para pruebas rápidas)')
    ap.add_argument('--salida', help='archivo de resultados; por defecto resultados/parrafos-<commit>-<fecha>.json')
    args = ap.parse_args()
    sys.stdout.reconfigure(line_buffering=True)

    noticias = [t for t in corpus.load(args.textos, {'noticia'}) if t['formato'] == 'html']
    if args.limite:
        noticias = noticias[:args.limite]
    articles = [{'id': t['id'], 'html': t['texto']} for t in noticias]

    commit = git('rev-parse', '--short', 'HEAD')
    sucio = bool(git('status', '--porcelain', '--untracked-files=no'))
    stamp = dt.datetime.now().strftime('%Y%m%d-%H%M%S')
    out_path = pathlib.Path(args.salida) if args.salida else HERE / 'resultados' / ('parrafos-%s-%s.json' % (commit, stamp))
    out_path.parent.mkdir(parents=True, exist_ok=True)

    print('Commit %s%s · modelo %s · %d noticias' % (commit, ' (con cambios sin confirmar)' if sucio else '', args.modelo, len(articles)))
    translated = translate_articles(articles, args.url, args.modelo)

    por_id = {t['id']: t for t in noticias}
    filas = []
    for r in translated:
        fuente = por_id[r['id']]
        porcentaje = round(100 * r['en_ingles'] / r['segmentos'], 1) if r['segmentos'] else None
        print('%-12s %3d párrafos · %2d en inglés · %s %%' % (r['id'], r['segmentos'], r['en_ingles'], porcentaje))
        filas.append({'id': r['id'], 'url': fuente['url'], 'segmentos': r['segmentos'],
                      'en_ingles': r['en_ingles'], 'html': r['html']})

    total = sum(f['segmentos'] for f in filas)
    en_ingles = sum(f['en_ingles'] for f in filas)
    resumen = {
        'segmentos': total,
        'en_ingles': en_ingles,
        'proporcion_en_ingles': round(en_ingles / total, 4) if total else None,
    }
    report = {
        'commit': commit,
        'con_cambios_sin_confirmar': sucio,
        'modelo': args.modelo,
        'url': args.url,
        'fecha': dt.datetime.now().astimezone().isoformat(timespec='seconds'),
        'noticias': filas,
        'resumen': resumen,
    }
    out_path.write_text(json.dumps(report, ensure_ascii=False, indent=1))
    print('\nTotal: %d de %d párrafos en inglés (%s)' % (en_ingles, total, resumen['proporcion_en_ingles']))
    print('Resultados en', out_path)


if __name__ == '__main__':
    main()
