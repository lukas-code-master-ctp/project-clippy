# Brote

Mascota de escritorio con comandos de voz. App de escritorio hecha con Tauri 2 (núcleo en Rust, interfaz en React con TypeScript).

## Estado actual: hito 4 (Gmail y Calendar)

- Brote flota en una ventana transparente, sin bordes y siempre encima de las demás apps. Aparece en la esquina inferior derecha y no roba el foco.
- **Arrastrar** a Brote lo mueve por la pantalla.
- **Clic** sobre Brote pasa al siguiente estado (demo): durmiendo, escuchando, pensando, trabajando, listo.
- **Clic derecho** abre un menú para elegir un estado, ocultarlo o salir.
- **Ctrl+Shift+Espacio** (mantener): Brote escucha y graba el micrófono. Al soltar, transcribe en tu computador con Whisper y muestra en un globo lo que entendió. El audio nunca sale del equipo.
- **Primer arranque:** Brote baja su modelo de voz (Whisper `small` cuantizado, unos 190 MB) a la carpeta de datos de la app. Es solo la primera vez y avisa el avance en el globo.
- **Ícono en la bandeja del sistema** con los mismos estados, mostrar u ocultar, y salir.

- **Agente:** lo que dijiste va a un modelo en OpenRouter (solo el texto, nunca el audio) y Brote responde en el globo. Recuerda la conversación por 3 minutos, así que puedes responderle ("el primero", "sí, a las 4").
- **Gmail:** buscar y leer correos, y enviar o responder en el mismo hilo. El correo de la persona sale de tus contactos o de quienes te han escrito, nunca del dictado; si hay dos Juan, Brote pregunta cuál.
- **Calendar:** ver la agenda y crear, mover o cancelar eventos. Si hay invitados, Google les avisa.
- **Herramientas extra:** fecha y hora, y abrir un sitio web.
- **Conectar Google:** requiere un proyecto propio en Google Cloud. Guía paso a paso: [docs/configurar-google.md](docs/configurar-google.md).
- **Confirmación obligatoria:** toda acción sensible (enviar un correo, crear, mover o cancelar un evento, abrir un sitio) muestra una tarjeta con exactamente lo que se va a hacer, y solo corre si apretas Confirmar. Puedes corregir el texto en la tarjeta y se ejecuta la versión corregida, después de volver a validarla. La regla está en el código (`src-tauri/src/agent/policy.rs`), no en el prompt: una herramienta sensible no puede ejecutarse sin una aprobación, y cada aprobación vale para una sola acción. Sin respuesta en 2 minutos cuenta como "no".
- **Ajustes** (clic derecho o bandeja): API key de OpenRouter, modelo y conexión con Google. La key se guarda en el Administrador de credenciales de Windows, nunca en un archivo. "Guardar y probar" valida la key y el modelo.
- **Detener** (clic derecho), o volver a hablar, corta lo que Brote esté haciendo.

El núcleo en Rust es la fuente de verdad del estado (`src-tauri/src/pet.rs`). La interfaz solo lo dibuja escuchando el evento `pet-state`. Así, el agente de voz podrá mover a Brote desde Rust sin tocar la interfaz.

## Probarlo en Windows sin instalar nada

Cada cambio en `main` compila el instalador en GitHub Actions. Entra a la pestaña **Actions** del repo, abre la última ejecución de "Instalador de Windows" y descarga el artefacto `brote-windows`. Trae un `.exe` (instalador) y un `.msi`.

Como el instalador todavía no está firmado, Windows SmartScreen puede advertir al abrirlo: elige "Más información" y luego "Ejecutar de todas formas".

## Requisitos

- [Node.js](https://nodejs.org) 20 o superior
- [Rust](https://rustup.rs) estable
- CMake y LLVM/Clang (para compilar whisper.cpp). En Windows: `winget install Kitware.CMake LLVM.LLVM`
- Dependencias de sistema de Tauri según tu sistema: https://v2.tauri.app/start/prerequisites/
  - **Windows:** Microsoft C++ Build Tools y WebView2 (ya viene en Windows 10 y 11).
  - **Linux:** además, `libasound2-dev` para el micrófono.
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

## Pruebas

```bash
cd src-tauri
cargo test                       # unitarias
# De punta a punta: baja el modelo real y transcribe un .wav en español
BROTE_TEST_WAV=frase.wav cargo test --release -- --ignored --nocapture
```

Variables útiles en desarrollo:

- `BROTE_WHISPER_MODEL=/ruta/modelo.bin`: usar otro modelo de Whisper.
- `BROTE_DEMO_COMMAND="abre el sitio del SII"`: ejecuta ese comando al arrancar, sin micrófono (solo en builds de desarrollo).
- `BROTE_OPENROUTER_URL=http://127.0.0.1:8787/`: apuntar el agente a un servidor de prueba.

## Estructura

```
src/                 Interfaz (React + TS)
  App.tsx            Ventana de Brote: arrastre, clic, menú contextual
  Bubble.tsx         Globo de texto (segunda ventana, no recibe clics)
  Confirm.tsx        Tarjeta de confirmación de acciones sensibles
  Settings.tsx       Ventana de ajustes
  GoogleSettings.tsx Sección de Google en Ajustes
  brote.svg          Personaje animado; el estado se cambia con la clase s-<estado>
  state.ts           Lista de estados y sus nombres en español
src-tauri/
  src/lib.rs         Ventana, bandeja, tecla global y comandos
  src/pet.rs         Estado de Brote (fuente de verdad)
  src/bubble.rs      Globo: qué dice, dónde va, cuándo se oculta
  src/voice/         Micrófono (cpal), modelo de Whisper y transcripción
  src/agent/         Agente: OpenRouter, herramientas, capa de políticas y tarjeta de confirmación
  src/settings.rs    Ajustes y API key (llavero del sistema)
  src/google/        OAuth de escritorio (PKCE), Gmail, Calendar, Contactos y sus herramientas
  tauri.conf.json    Ventana transparente, siempre encima, sin barra de tareas
```

## Próximos hitos

1. ~~Ventana flotante con Brote y sus estados~~
2. ~~Voz: grabar mientras se mantiene la tecla y transcribir en local con Whisper (whisper.cpp)~~
3. ~~Agente con OpenRouter y capa de políticas (confirmación obligatoria en código)~~
4. ~~Gmail y Google Calendar, con tarjetas de confirmación~~
5. Historial local de comandos y acciones (R11)
