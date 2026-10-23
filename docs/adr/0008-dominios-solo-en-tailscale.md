# 0008. Dominios propios, solo dentro de Tailscale

- **Estado:** aceptada
- **Fecha:** 2026-10-23

## Contexto

Android bloquea por defecto las conexiones sin HTTPS. En `ai-lab`, todo entra por Caddy con un dominio `*.rubensalas.dev`, que pone HTTPS solo.

## Decisión

| Dominio | Qué hay detrás |
|---|---|
| `rss.rubensalas.dev` | La web y la API de Miniflux |
| `atalaya.rubensalas.dev` | La API del servicio. No muestra ninguna web |

Los dos se ven solo dentro de Tailscale, como Vaultwarden. No salen a internet por Cloudflare Tunnel.

## Consecuencias

- Las apps funcionan desde cualquier sitio con Tailscale activo.
- Miniflux y el servicio se hablan por la red interna de Docker, sin pasar por los dominios.
