# Conectar Brote con Gmail y Google Calendar

Brote usa las APIs oficiales de Google. Para eso necesita una "app" registrada en Google Cloud a tu nombre. Se hace una sola vez y toma unos 10 minutos. No tiene costo.

Al final vas a tener dos datos, un **Client ID** y un **Client secret**, que pegas en **Ajustes de Brote → Google**.

> Los nombres de los menús de Google Cloud cambian seguido. Si algo no calza exacto, busca la opción con el nombre parecido.

## 1. Crear el proyecto

1. Entra a [console.cloud.google.com](https://console.cloud.google.com) con la cuenta de Google que quieres que use Brote.
2. Arriba a la izquierda, abre el selector de proyectos y elige **Proyecto nuevo**.
3. Nombre: `Brote`. Si tu cuenta es de una empresa (Google Workspace), deja la organización que aparece. Aprieta **Crear** y espera a que termine.
4. Asegúrate de que el proyecto `Brote` quede seleccionado arriba.

## 2. Activar las tres APIs

Abre cada enlace, revisa que arriba diga el proyecto `Brote` y aprieta **Habilitar**:

- [Gmail API](https://console.cloud.google.com/apis/library/gmail.googleapis.com)
- [Google Calendar API](https://console.cloud.google.com/apis/library/calendar-json.googleapis.com)
- [People API](https://console.cloud.google.com/apis/library/people.googleapis.com) (para buscar contactos)

## 3. Configurar la pantalla de permisos

1. Ve a [Google Auth Platform](https://console.cloud.google.com/auth/overview) y aprieta **Comenzar** (o **Get started**).
2. **Información de la app:** nombre `Brote` y tu correo como correo de asistencia.
3. **Público** (Audience). Aquí hay dos caminos:
   - **Si tu cuenta es de Google Workspace** (un correo de tu empresa, no @gmail.com) y el proyecto quedó dentro de la organización: elige **Interno**. Es lo mejor: no hay advertencias, no hay que agregar usuarios de prueba y el permiso no vence.
   - **Si es una cuenta @gmail.com:** elige **Externo**.
4. **Información de contacto:** tu correo. Acepta las condiciones y aprieta **Crear**.
5. Solo si elegiste **Externo**: entra a **Público** y, en **Usuarios de prueba**, agrega tu propio correo.

## 4. Crear las credenciales

1. Ve a [Clientes](https://console.cloud.google.com/auth/clients) y aprieta **Crear cliente**.
2. **Tipo de aplicación:** `App de escritorio` (Desktop app). Nombre: `Brote`.
3. Aprieta **Crear**. Aparecen el **ID de cliente** y el **Secreto del cliente**.
4. Cópialos ahora o descarga el JSON: después Google puede no volver a mostrar el secreto.

## 5. Conectar en Brote

1. Clic derecho sobre Brote → **Ajustes…**
2. En la sección **Google**, pega el **Client ID** y el **Client secret** y aprieta **Guardar credenciales**.
3. Aprieta **Conectar Google**. Se abre tu navegador con la pantalla de Google.
4. Si elegiste **Externo**, Google avisa que "no verificó esta app". Es normal: la app es tuya. Aprieta **Continuar**.
5. Marca **todas** las casillas de permisos y acepta. El navegador dice "Listo" y Ajustes muestra "Conectado como tu@correo".

Prueba con: *"¿Qué tengo mañana en el calendario?"*

## Qué permisos pide y por qué

| Permiso | Para qué |
|---|---|
| Leer tus correos | Buscar y leer correos cuando se lo pides |
| Enviar correos en tu nombre | Enviar, siempre después de que apruebes la tarjeta |
| Ver y editar eventos | Consultar, crear, mover y cancelar eventos (cambiar algo también pide tarjeta) |
| Ver contactos | Encontrar el correo de una persona por su nombre |

El token queda en el Administrador de credenciales de Windows. Puedes desconectar desde Ajustes o desde [tu cuenta de Google](https://myaccount.google.com/connections).

## Problemas comunes

- **"Acceso bloqueado" o "app no autorizada":** con **Externo**, falta agregar tu correo en **Usuarios de prueba** (paso 3.5).
- **"Google no dio permiso para esto":** falta habilitar alguna de las tres APIs (paso 2), o no marcaste todas las casillas al conectar. Desconecta y vuelve a conectar.
- **Cada 7 días hay que reconectar:** pasa con **Externo** en modo de prueba. Para evitarlo, en [Público](https://console.cloud.google.com/auth/audience) aprieta **Publicar app**. Para uso personal no hace falta pasar la verificación de Google; solo vas a seguir viendo el aviso de "app no verificada" al conectar. Esto no aplica con **Interno**.
