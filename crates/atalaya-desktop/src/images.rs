//! Cliente HTTP con el que GPUI baja las imágenes de los artículos (#18).
//!
//! Sin él, GPUI usa uno vacío y cada imagen falla con «No HttpClient
//! available». Usa el mismo `reqwest` bloqueante que el cliente del
//! servicio: cada descarga va en un hilo propio y el resultado vuelve por un
//! canal, para no frenar la interfaz.
//!
//! Las figuras de los papers suelen ser PNG o SVG transparentes con texto
//! negro, ilegibles en tema oscuro (#100). Por eso cada imagen bajada se
//! compone sobre una tarjeta blanca antes de llegar a GPUI. Las fórmulas no
//! pasan por aquí: van por el servicio (`fulltext::formulas`).

use std::io::Cursor;
use std::ops::Range;
use std::time::Duration;

use futures::channel::oneshot;
use futures::future::BoxFuture;
use gpui::http_client::http::HeaderValue;
use gpui::http_client::http::header::CONTENT_LENGTH;
use gpui::http_client::{self, AsyncBody, HttpClient, Request, Response, Url};
use image::{DynamicImage, ImageFormat, Rgba, RgbaImage};

pub struct ImageClient {
    http: reqwest::blocking::Client,
}

impl ImageClient {
    pub fn new() -> Option<Self> {
        let http = reqwest::blocking::Client::builder()
            .timeout(Duration::from_secs(30))
            // Algunos servidores de imágenes (Wikimedia) rechazan las
            // peticiones sin User-Agent.
            .user_agent(concat!("Atalaya/", env!("CARGO_PKG_VERSION")))
            .build()
            .ok()?;
        Some(Self { http })
    }
}

impl HttpClient for ImageClient {
    fn user_agent(&self) -> Option<&HeaderValue> {
        None
    }

    fn proxy(&self) -> Option<&Url> {
        None
    }

    /// Solo se usa para bajar imágenes, así que el cuerpo de la petición
    /// no se manda. Las imágenes que se saben componer salen sobre blanco
    /// (#100); el resto, tal cual.
    fn send(
        &self,
        request: Request<AsyncBody>,
    ) -> BoxFuture<'static, http_client::Result<Response<AsyncBody>>> {
        let http = self.http.clone();
        let (parts, _) = request.into_parts();
        let (tx, rx) = oneshot::channel();
        std::thread::spawn(move || {
            let result = (|| -> http_client::Result<Response<AsyncBody>> {
                let response = http
                    .request(parts.method, parts.uri.to_string())
                    .headers(parts.headers)
                    .send()?;
                let status = response.status();
                let mut builder = Response::builder().status(status);
                for (name, value) in response.headers() {
                    // La longitud cambia al componer la imagen.
                    if name != CONTENT_LENGTH {
                        builder = builder.header(name, value);
                    }
                }
                let bytes = response.bytes()?;
                let body = if status.is_success() {
                    on_white_card(&bytes).unwrap_or_else(|| bytes.to_vec())
                } else {
                    bytes.to_vec()
                };
                Ok(builder.body(AsyncBody::from(body))?)
            })();
            tx.send(result).ok();
        });
        Box::pin(async move { rx.await? })
    }
}

/// La imagen sobre una tarjeta blanca, con un poco de margen y las esquinas
/// redondeadas (#100). `None` si no es un PNG, un JPEG ni un SVG que sepamos
/// componer (un GIF animado o un WebP salen como llegan).
pub fn on_white_card(bytes: &[u8]) -> Option<Vec<u8>> {
    on_white_card_raster(bytes).or_else(|| on_white_card_svg(bytes))
}

fn on_white_card_raster(bytes: &[u8]) -> Option<Vec<u8>> {
    let format = image::guess_format(bytes).ok()?;
    if !matches!(format, ImageFormat::Png | ImageFormat::Jpeg) {
        return None;
    }
    let source = image::load_from_memory_with_format(bytes, format)
        .ok()?
        .to_rgba8();
    let mut out = Cursor::new(Vec::new());
    DynamicImage::ImageRgba8(card_pixels(&source))
        .write_to(&mut out, ImageFormat::Png)
        .ok()?;
    Some(out.into_inner())
}

/// Pinta `source` sobre blanco: lo transparente queda blanco, con un margen
/// blanco alrededor. Fuera de las esquinas redondeadas queda transparente,
/// para que se vea el fondo del tema.
fn card_pixels(source: &RgbaImage) -> RgbaImage {
    let (w, h) = source.dimensions();
    let margin = margin_for(w.min(h) as f32).round() as u32;
    let (cw, ch) = (w + 2 * margin, h + 2 * margin);
    let radius = (2 * margin).min(cw / 2).min(ch / 2);
    let mut card = RgbaImage::from_pixel(cw, ch, Rgba([255, 255, 255, 255]));
    for (x, y, pixel) in source.enumerate_pixels() {
        let alpha = pixel.0[3] as f32 / 255.0;
        let target = card.get_pixel_mut(x + margin, y + margin);
        for (out, &src) in target.0.iter_mut().take(3).zip(pixel.0.iter()) {
            *out = (src as f32 * alpha + 255.0 * (1.0 - alpha)).round() as u8;
        }
    }
    for (x, y, pixel) in card.enumerate_pixels_mut() {
        if outside_corner(x, y, cw, ch, radius) {
            pixel.0[3] = 0;
        }
    }
    card
}

/// El margen de la tarjeta: un 2 % del lado corto, y al menos 4 píxeles.
fn margin_for(short_side: f32) -> f32 {
    (short_side * 0.02).max(4.0)
}

/// Si el píxel (`x`, `y`) cae fuera de un rectángulo `w`×`h` con esquinas de
/// radio `r`.
fn outside_corner(x: u32, y: u32, w: u32, h: u32, r: u32) -> bool {
    if r == 0 {
        return false;
    }
    let (x, y, r) = (x as f32 + 0.5, y as f32 + 0.5, r as f32);
    let (w, h) = (w as f32, h as f32);
    let cx = if x < r {
        r
    } else if x > w - r {
        w - r
    } else {
        return false;
    };
    let cy = if y < r {
        r
    } else if y > h - r {
        h - r
    } else {
        return false;
    };
    (x - cx).powi(2) + (y - cy).powi(2) > r * r
}

/// Un SVG con un rectángulo blanco debajo del dibujo. Si tiene `viewBox`, se
/// agranda un poco para que el dibujo quede con margen dentro del rectángulo.
fn on_white_card_svg(bytes: &[u8]) -> Option<Vec<u8>> {
    let text = std::str::from_utf8(bytes).ok()?;
    let start = text.find("<svg")?;
    let end = start + text[start..].find('>')? + 1;
    let tag = &text[start..end];
    if tag.ends_with("/>") {
        return None;
    }
    let (tag, rect) = match view_box(tag) {
        Some((x, y, w, h)) => {
            let m = (w.min(h) * 0.02).max(0.5);
            let (x, y, w, h) = (x - m, y - m, w + 2.0 * m, h + 2.0 * m);
            let tag = replace_view_box(tag, &format!("{x} {y} {w} {h}"))?;
            let rect = format!(
                r##"<rect x="{x}" y="{y}" width="{w}" height="{h}" rx="{}" fill="#ffffff"/>"##,
                2.0 * m
            );
            (tag, rect)
        }
        None => (
            tag.to_owned(),
            r##"<rect width="100%" height="100%" fill="#ffffff"/>"##.to_owned(),
        ),
    };
    Some(format!("{}{tag}{rect}{}", &text[..start], &text[end..]).into_bytes())
}

/// El `viewBox` de la etiqueta `<svg>`, si tiene cuatro números y un tamaño.
fn view_box(tag: &str) -> Option<(f32, f32, f32, f32)> {
    let range = attr_range(tag, "viewBox")?;
    let nums = tag[range]
        .split(|c: char| c.is_whitespace() || c == ',')
        .filter(|s| !s.is_empty())
        .map(|s| s.parse::<f32>().ok())
        .collect::<Option<Vec<_>>>()?;
    match nums[..] {
        [x, y, w, h] if w > 0.0 && h > 0.0 => Some((x, y, w, h)),
        _ => None,
    }
}

fn replace_view_box(tag: &str, value: &str) -> Option<String> {
    let range = attr_range(tag, "viewBox")?;
    Some(format!(
        "{}{value}{}",
        &tag[..range.start],
        &tag[range.end..]
    ))
}

/// Dónde está el valor entre comillas del atributo `name` en `tag`.
fn attr_range(tag: &str, name: &str) -> Option<Range<usize>> {
    let key = format!("{name}=\"");
    let from = tag.find(&key)? + key.len();
    let to = from + tag[from..].find('"')?;
    Some(from..to)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn encode_png(image: &RgbaImage) -> Vec<u8> {
        let mut out = Cursor::new(Vec::new());
        DynamicImage::ImageRgba8(image.clone())
            .write_to(&mut out, ImageFormat::Png)
            .unwrap();
        out.into_inner()
    }

    fn decode(bytes: &[u8]) -> RgbaImage {
        image::load_from_memory(bytes).unwrap().to_rgba8()
    }

    #[test]
    fn la_transparencia_de_un_png_queda_blanca_con_margen() {
        // 10x10 transparente, con un píxel rojo opaco en (2, 2).
        let mut source = RgbaImage::from_pixel(10, 10, Rgba([0, 0, 0, 0]));
        source.put_pixel(2, 2, Rgba([255, 0, 0, 255]));
        let card = decode(&on_white_card(&encode_png(&source)).unwrap());

        // El margen mínimo es de 4 píxeles: la tarjeta mide 18x18.
        assert_eq!(card.dimensions(), (18, 18));
        // Lo transparente se ve blanco, y el borde de la tarjeta es blanco.
        assert_eq!(card.get_pixel(9, 9).0, [255, 255, 255, 255]);
        assert_eq!(card.get_pixel(4, 4).0, [255, 255, 255, 255]);
        // El píxel opaco conserva su color, desplazado por el margen.
        assert_eq!(card.get_pixel(6, 6).0, [255, 0, 0, 255]);
    }

    #[test]
    fn un_pixel_semitransparente_se_mezcla_con_el_blanco() {
        let source = RgbaImage::from_pixel(10, 10, Rgba([0, 0, 0, 128]));
        let card = decode(&on_white_card(&encode_png(&source)).unwrap());
        let gray = card.get_pixel(9, 9).0;
        // Negro al 50 % sobre blanco: gris medio.
        assert!((126..=129).contains(&gray[0]), "{gray:?}");
        assert_eq!(gray[0], gray[1]);
        assert_eq!(gray[0], gray[2]);
    }

    #[test]
    fn las_esquinas_quedan_redondeadas() {
        let source = RgbaImage::from_pixel(40, 40, Rgba([0, 0, 0, 255]));
        let card = decode(&on_white_card(&encode_png(&source)).unwrap());
        let (w, h) = card.dimensions();
        // La esquina exacta queda fuera de la tarjeta, transparente.
        assert_eq!(card.get_pixel(0, 0).0[3], 0);
        assert_eq!(card.get_pixel(w - 1, h - 1).0[3], 0);
        // El centro de cada lado sigue dentro.
        assert_eq!(card.get_pixel(w / 2, 0).0[3], 255);
        assert_eq!(card.get_pixel(0, h / 2).0[3], 255);
    }

    #[test]
    fn lo_que_no_es_una_imagen_se_deja_como_esta() {
        assert!(on_white_card(b"<html><body>no</body></html>").is_none());
        assert!(on_white_card(b"").is_none());
    }

    #[test]
    fn un_svg_con_viewbox_gana_un_rectangulo_blanco_con_margen() {
        let svg = br#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 100 50"><path d="M0 0" stroke="black"/></svg>"#;
        let out = String::from_utf8(on_white_card(svg).unwrap()).unwrap();
        // El 2 % de 50 es 1: el viewBox crece un píxel por cada lado.
        assert!(out.contains(r#"viewBox="-1 -1 102 52""#), "{out}");
        let rect =
            out.find(r##"<rect x="-1" y="-1" width="102" height="52" rx="2" fill="#ffffff"/>"##);
        let path = out.find("<path");
        // El blanco va detrás del dibujo.
        assert!(rect.is_some() && rect < path, "{out}");
    }

    #[test]
    fn un_svg_sin_viewbox_usa_todo_el_lienzo() {
        let svg = br#"<svg xmlns="http://www.w3.org/2000/svg" width="10" height="10"><circle r="3"/></svg>"#;
        let out = String::from_utf8(on_white_card(svg).unwrap()).unwrap();
        assert!(
            out.contains(r##"<rect width="100%" height="100%" fill="#ffffff"/>"##),
            "{out}"
        );
    }
}
