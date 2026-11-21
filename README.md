# Brote

Mascota de escritorio con comandos de voz. App de escritorio hecha con Tauri 2 (núcleo en Rust, interfaz en React con TypeScript).

## Estado actual: hito 1

- Brote flota en una ventana transparente, sin bordes y siempre encima de las demás apps. Aparece en la esquina inferior derecha y no roba el foco.
- **Arrastrar** a Brote lo mueve por la pantalla.
- **Clic** sobre Brote pasa al siguiente estado (demo): durmiendo, escuchando, pensando, trabajando, listo.
- **Clic derecho** abre un menú para elegir un estado, ocultarlo o salir.
- **Ctrl+Shift+Espacio** (mantener): Brote escucha. Al soltar, simula el flujo completo: pensando, trabajando, listo y de vuelta a dormir. Todavía no graba audio: eso llega en el hito 2.
- **Ícono en la bandeja del sistema** con los mismos estados, mostrar u ocultar, y salir.

El núcleo en Rust es la fuente de verdad del estado (`src-tauri/src/pet.rs`). La interfaz solo lo dibuja escuchando el evento `pet-state`. Así, el agente de voz podrá mover a Brote desde Rust sin tocar la interfaz.

## Probarlo en Windows sin instalar nada

Cada cambio en `main` compila el instalador en GitHub Actions. Entra a la pestaña **Actions** del repo, abre la última ejecución de "Instalador de Windows" y descarga el artefacto `brote-windows`. Trae un `.exe` (instalador) y un `.msi`.

Como el instalador todavía no está firmado, Windows SmartScreen puede advertir al abrirlo: elige "Más información" y luego "Ejecutar de todas formas".

## Requisitos

- [Node.js](https://nodejs.org) 20 o superior
- [Rust](https://rustup.rs) estable
- Dependencias de sistema de Tauri según tu sistema: https://v2.tauri.app/start/prerequisites/
  - **Windows:** Microsoft C++ Build Tools y WebView2 (ya viene en Windows 10 y 11).
  - **macOS:** Xcode Command Line Tools (`xcode-select --install`).

## Correr en modo desarrollo

```bash
npm install
npm run tauri dev
```

La primera compilación de Rust tarda unos minutos. Las siguientes son rápidas.

## Generar el instalador

```bash
npm run tauri build
```

El instalador queda en `src-tauri/target/release/bundle/`.

## Estructura

```
src/                 Interfaz (React + TS)
  App.tsx            Ventana de Brote: arrastre, clic, menú contextual
  brote.svg          Personaje animado; el estado se cambia con la clase s-<estado>
  state.ts           Lista de estados y sus nombres en español
src-tauri/
  src/lib.rs         Ventana, bandeja, tecla global y comandos
  src/pet.rs         Estado de Brote (fuente de verdad)
  tauri.conf.json    Ventana transparente, siempre encima, sin barra de tareas
```

## Próximos hitos

1. ~~Ventana flotante con Brote y sus estados~~
2. Voz: grabar mientras se mantiene la tecla y transcribir en local con Whisper (whisper.cpp)
3. Agente con OpenRouter y capa de políticas (confirmación obligatoria en código)
4. Gmail y Google Calendar, con tarjetas de confirmación
