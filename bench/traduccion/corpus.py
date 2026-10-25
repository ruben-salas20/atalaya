"""Carga el conjunto de prueba.

El repositorio es público y las noticias son de otros medios, así que
`textos.json` guarda de ellas solo el título y el enlace. El texto completo
está en `textos-completos.json`, en esta misma carpeta, que git ignora. En
ai-lab hay una copia en `/data/bots/atalaya-datos/textos-completos.json`.
"""
import json
import pathlib
import sys

HERE = pathlib.Path(__file__).resolve().parent
FULL = HERE / 'textos-completos.json'
PUBLIC = HERE / 'textos.json'


def default_path():
    """El archivo completo si existe; si no, el público."""
    return FULL if FULL.exists() else PUBLIC


def load(path, kinds=None):
    """Lee los textos de `path`, solo de los tipos de `kinds` si se da.

    Sale con un aviso si a alguno le falta el texto.
    """
    corpus = json.loads(pathlib.Path(path).read_text())['textos']
    if kinds is not None:
        corpus = [t for t in corpus if t['tipo'] in kinds]
    missing = [t['id'] for t in corpus if 'texto' not in t]
    if missing:
        sys.exit('Faltan los textos de %s en %s. Copia textos-completos.json en %s '
                 '(en ai-lab: /data/bots/atalaya-datos/) o pasa --tipos sin noticias.'
                 % (', '.join(missing), path, HERE))
    return corpus
