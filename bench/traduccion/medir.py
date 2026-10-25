#!/usr/bin/env python3
"""Mide cuánto tarda un modelo de llama-swap en traducir el conjunto de prueba.

Uso:
    python3 medir.py MODELO [--plantilla NOMBRE] [--tipos noticia,resumen,titulo]
                            [--limite N] [--modo parrafos|entero] [--descargar-antes]

Habla la API OpenAI /v1/chat/completions de llama-swap. Guarda un JSON en
resultados/ con el tiempo de la primera petición (incluye la carga del modelo)
y, por cada texto, su tiempo, sus tokens y su traducción.
Solo usa la biblioteca estándar de Python.
"""
import argparse
import datetime as dt
import json
import pathlib
import re
import sys
import time
import urllib.error
import urllib.request

import corpus

HERE = pathlib.Path(__file__).resolve().parent
DEFAULT_URL = 'https://llama.rubensalas.dev'
WARMUP_TEXT = 'Hello, world.'
PARAGRAPH_RE = re.compile(r'<p>(.*?)</p>', re.S)


def http_json(url, payload=None, timeout=900):
    data = None if payload is None else json.dumps(payload).encode()
    req = urllib.request.Request(url, data=data, headers={'Content-Type': 'application/json'})
    with urllib.request.urlopen(req, timeout=timeout) as resp:
        body = resp.read()
    return json.loads(body) if body.strip() else {}


def pick_template(templates, model, name):
    if name:
        if name not in templates:
            sys.exit('No existe la plantilla %r. Hay: %s' % (name, ', '.join(k for k in templates if not k.startswith('_'))))
        return name, templates[name]
    for key, tpl in templates.items():
        if not key.startswith('_') and key != 'generica' and key in model.lower():
            return key, tpl
    return 'generica', templates['generica']


def translate(base_url, model, tpl, text, is_html, max_tokens):
    prompt = (tpl.get('user_html') if is_html else None) or tpl['user']
    messages = []
    if tpl.get('system'):
        messages.append({'role': 'system', 'content': tpl['system']})
    messages.append({'role': 'user', 'content': prompt.replace('{texto}', text)})
    payload = {'model': model, 'messages': messages, 'stream': False, 'max_tokens': max_tokens}
    payload.update(tpl.get('parametros', {}))
    t0 = time.perf_counter()
    resp = http_json(base_url + '/v1/chat/completions', payload)
    seconds = time.perf_counter() - t0
    usage = resp.get('usage') or {}
    timings = resp.get('timings') or {}
    choice = (resp.get('choices') or [{}])[0]
    return {
        'segundos': round(seconds, 3),
        'tokens_generados': usage.get('completion_tokens'),
        'tokens_entrada': usage.get('prompt_tokens'),
        'tokens_por_segundo_generacion': timings.get('predicted_per_second'),
        'fin': choice.get('finish_reason'),
        'traduccion': (choice.get('message') or {}).get('content', ''),
    }


def segments(item, mode):
    if item['formato'] == 'html' and mode == 'parrafos':
        return PARAGRAPH_RE.findall(item['texto'])
    return [item['texto']]


def sum_or_none(values):
    values = [v for v in values if v is not None]
    return sum(values) if values else None


def summarize(results):
    summary = {}
    for kind in ('titulo', 'resumen', 'noticia'):
        rows = [r for r in results if r['tipo'] == kind and 'error' not in r]
        if not rows:
            continue
        secs = [r['segundos'] for r in rows]
        toks = sum_or_none(r['tokens_generados'] for r in rows)
        summary[kind] = {
            'textos': len(rows),
            'segundos_total': round(sum(secs), 2),
            'segundos_media': round(sum(secs) / len(secs), 2),
            'segundos_max': round(max(secs), 2),
            'tokens_generados': toks,
            'tokens_por_segundo': round(toks / sum(secs), 1) if toks else None,
        }
    return summary


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument('modelo', help='id del modelo en llama-swap, p. ej. gemma-4-e4b-rapido')
    ap.add_argument('--plantilla', help='clave de plantillas.json; por defecto se deduce del id del modelo')
    ap.add_argument('--url', default=DEFAULT_URL, help='base de llama-swap (por defecto %(default)s)')
    ap.add_argument('--textos', default=str(corpus.default_path()))
    ap.add_argument('--tipos', default='titulo,resumen,noticia', help='qué tipos medir, separados por comas')
    ap.add_argument('--limite', type=int, help='como mucho N textos de cada tipo (para pruebas rápidas)')
    ap.add_argument('--modo', choices=('parrafos', 'entero'), default='parrafos',
                    help='las noticias van párrafo a párrafo (como hará el servicio) o enteras')
    ap.add_argument('--max-tokens', type=int, default=4096)
    ap.add_argument('--descargar-antes', action='store_true',
                    help='pide a llama-swap que descargue los modelos, para medir la carga en frío')
    ap.add_argument('--salida', help='archivo de resultados; por defecto resultados/<modelo>-<fecha>.json')
    args = ap.parse_args()
    sys.stdout.reconfigure(line_buffering=True)  # progreso visible aunque la salida vaya a un archivo

    base = args.url.rstrip('/')
    templates = json.loads((HERE / 'plantillas.json').read_text())
    tpl_name, tpl = pick_template(templates, args.modelo, args.plantilla)
    kinds = [k.strip() for k in args.tipos.split(',') if k.strip()]
    texts = corpus.load(args.textos, kinds)
    items = []
    for kind in kinds:
        chosen = [t for t in texts if t['tipo'] == kind]
        items += chosen[:args.limite] if args.limite else chosen

    stamp = dt.datetime.now().strftime('%Y%m%d-%H%M%S')
    safe_model = re.sub(r'[^A-Za-z0-9._-]+', '_', args.modelo)
    out_path = pathlib.Path(args.salida) if args.salida else HERE / 'resultados' / ('%s-%s.json' % (safe_model, stamp))
    out_path.parent.mkdir(parents=True, exist_ok=True)

    if args.descargar_antes:
        urllib.request.urlopen(base + '/unload', timeout=120).read()
        print('Modelos descargados de llama-swap.')
    try:
        running = [r.get('model') for r in http_json(base + '/running').get('running', [])]
    except (urllib.error.URLError, ValueError):
        running = None

    report = {
        'modelo': args.modelo,
        'url': base,
        'plantilla': {'nombre': tpl_name, **tpl},
        'modo': args.modo,
        'inicio': dt.datetime.now().astimezone().isoformat(timespec='seconds'),
        'modelos_cargados_al_empezar': running,
        'primera_peticion': None,
        'textos': [],
        'resumen': {},
    }

    def save():
        report['resumen'] = summarize(report['textos'])
        out_path.write_text(json.dumps(report, ensure_ascii=False, indent=1))

    print('Modelo %s · plantilla %s · %d textos · salida %s' % (args.modelo, tpl_name, len(items), out_path))
    first = translate(base, args.modelo, tpl, WARMUP_TEXT, False, args.max_tokens)
    first['texto'] = WARMUP_TEXT
    first['modelo_ya_cargado'] = None if running is None else args.modelo in running
    report['primera_peticion'] = first
    print('Primera petición (con carga): %.1f s' % first['segundos'])
    save()

    failures = 0
    for n, item in enumerate(items, 1):
        segs = segments(item, args.modo)
        is_html = item['formato'] == 'html'
        row = {k: item[k] for k in ('id', 'tipo', 'fuente', 'url')}
        row['caracteres'] = len(item['texto'])
        row['segmentos'] = len(segs)
        try:
            parts = [translate(base, args.modelo, tpl, s, is_html, args.max_tokens) for s in segs]
        except (urllib.error.URLError, TimeoutError, ValueError) as exc:
            failures += 1
            row['error'] = str(exc)
            report['textos'].append(row)
            save()
            print('[%d/%d] %s ERROR: %s' % (n, len(items), item['id'], exc))
            if failures >= 3:
                sys.exit('Tres errores seguidos: paro. Resultados parciales en %s' % out_path)
            continue
        failures = 0
        row['segundos'] = round(sum(p['segundos'] for p in parts), 3)
        row['tokens_generados'] = sum_or_none(p['tokens_generados'] for p in parts)
        row['tokens_entrada'] = sum_or_none(p['tokens_entrada'] for p in parts)
        row['cortados'] = sum(1 for p in parts if p['fin'] == 'length')
        if is_html and len(segs) > 1:
            row['traduccion'] = '\n'.join('<p>%s</p>' % p['traduccion'].strip() for p in parts)
            row['segundos_por_segmento'] = [p['segundos'] for p in parts]
        elif is_html and args.modo == 'parrafos':
            row['traduccion'] = '<p>%s</p>' % parts[0]['traduccion'].strip()
        else:
            row['traduccion'] = parts[0]['traduccion'].strip()
        report['textos'].append(row)
        save()
        print('[%d/%d] %-11s %7.1f s  %5s tokens  %s' % (
            n, len(items), item['id'], row['segundos'], row['tokens_generados'], row['traduccion'][:60].replace('\n', ' ')))

    report['fin'] = dt.datetime.now().astimezone().isoformat(timespec='seconds')
    save()
    print('\nResumen:')
    for kind, s in report['resumen'].items():
        print('  %-8s %2d textos · media %6.2f s · máx %6.2f s · %s tok/s' % (
            kind, s['textos'], s['segundos_media'], s['segundos_max'], s['tokens_por_segundo']))
    print('Resultados en', out_path)


if __name__ == '__main__':
    main()
