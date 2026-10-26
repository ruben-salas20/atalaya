//! Traduce artículos con el mismo camino que el servicio (`split`,
//! `translate_segment`, `assemble`), sin Miniflux ni la base de datos.
//! Lo usa `bench/traduccion/medir-parrafos.py`.
//!
//! Lee de la entrada estándar una lista `[{"id": …, "html": …}]` y escribe
//! otra con, por artículo, los párrafos traducidos (`segmentos`), los que
//! quedaron en inglés (`en_ingles`), el HTML final (`html`) y, por párrafo,
//! los segundos desde que empezó el artículo hasta que estuvo listo
//! (`segundos`, #32).
//!
//! Uso: `cargo run -q -p atalaya-server --example traducir_parrafos -- --url URL --model MODELO < entrada.json`

use std::io::Read;

use atalaya_server::{
    content::{Part, assemble, split, translate_segment},
    llama::Llama,
};
use serde::{Deserialize, Serialize};

#[derive(Deserialize)]
struct Articulo {
    id: String,
    html: String,
}

#[derive(Serialize)]
struct Resultado {
    id: String,
    segmentos: usize,
    en_ingles: usize,
    html: String,
    segundos: Vec<f32>,
}

#[tokio::main]
async fn main() -> Result<(), String> {
    let mut url = "https://llama.rubensalas.dev".to_owned();
    let mut model = "gemma-4-e4b-rapido".to_owned();
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        let value = args.next().ok_or(format!("falta el valor de {arg}"))?;
        match arg.as_str() {
            "--url" => url = value,
            "--model" => model = value,
            _ => return Err(format!("opción desconocida: {arg}")),
        }
    }

    let mut input = String::new();
    std::io::stdin()
        .read_to_string(&mut input)
        .map_err(|e| e.to_string())?;
    let articulos: Vec<Articulo> = serde_json::from_str(&input).map_err(|e| e.to_string())?;

    let llama = Llama::new(reqwest::Client::new(), &url, &model);
    let mut salida = Vec::new();
    for articulo in articulos {
        let parts = split(&articulo.html);
        let mut traducidos = Vec::new();
        let (mut segmentos, mut en_ingles) = (0, 0);
        let started = std::time::Instant::now();
        let mut segundos = Vec::new();
        for part in &parts {
            if let Part::Translate(text) = part {
                let segment = translate_segment(&llama, text).await?;
                segmentos += 1;
                if !segment.translated {
                    en_ingles += 1;
                }
                traducidos.push(segment.html);
                segundos.push(started.elapsed().as_secs_f32());
            }
        }
        eprintln!(
            "{}: {segmentos} párrafos, {en_ingles} en inglés",
            articulo.id
        );
        salida.push(Resultado {
            id: articulo.id,
            segmentos,
            en_ingles,
            html: assemble(&parts, &traducidos),
            segundos,
        });
    }
    println!(
        "{}",
        serde_json::to_string(&salida).map_err(|e| e.to_string())?
    );
    Ok(())
}
