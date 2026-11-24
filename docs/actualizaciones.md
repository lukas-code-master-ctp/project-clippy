# Actualizaciones automáticas

Cada vez que se mergea un PR a `main`, GitHub Actions (`.github/workflows/release.yml`) compila Brote,
lo **firma** y lo publica en [Releases](https://github.com/lukas-code-master-ctp/project-clippy/releases)
como versión `0.2.N`. Brote revisa `latest.json` al arrancar y cada 6 horas; si hay una versión nueva,
la baja, espera a que no lo estés usando y se reinstala solo. También se puede forzar desde
**Ajustes → Actualizaciones → Buscar actualizaciones**.

Brote solo instala archivos firmados con la llave de este proyecto: la llave pública está en
`src-tauri/tauri.conf.json` y la privada vive solo en un secret de GitHub.

## Configuración (una sola vez)

1. **Repo público.** En GitHub: *Settings → General →* al final, *Danger Zone → Change repository
   visibility → Public*. Brote descarga las versiones sin iniciar sesión, así que Releases tiene que ser público.
2. **Llave de firma.** *Settings → Secrets and variables → Actions → New repository secret*:
   - Name: `TAURI_SIGNING_PRIVATE_KEY`
   - Secret: el contenido completo del archivo de la llave privada (una línea larga).
   Guarda también una copia en tu gestor de contraseñas: GitHub no deja volver a leer un secret, y sin
   esta llave no se pueden firmar versiones que las copias instaladas acepten.
3. **Primera instalación a mano.** Después del primer merge, baja `Brote_0.2.N_x64-setup.exe` desde
   Releases e instálalo. Desde ahí en adelante se actualiza solo.

## Si algo falla

- El workflow "Publicar versión" falla con *Falta el secret TAURI_SIGNING_PRIVATE_KEY*: falta el paso 2.
- Brote no se actualiza: revisa `brote.log` (Ajustes → Abrir carpeta de logs), líneas `Actualización:`.
- Si se pierde la llave privada: generar una nueva (`npx tauri signer generate`), cambiar la pública en
  `tauri.conf.json`, y reinstalar Brote a mano una vez.
