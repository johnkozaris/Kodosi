# Primeros pasos

[English](../GETTING_STARTED.md) · [Deutsch](../de/GETTING_STARTED.md) · [Ελληνικά](../el/GETTING_STARTED.md) · **Español** · [Français](../fr/GETTING_STARTED.md) · [Italiano](../it/GETTING_STARTED.md) · [日本語](../ja/GETTING_STARTED.md) · [한국어](../ko/GETTING_STARTED.md) · [简体中文](../zh-Hans/GETTING_STARTED.md)

[Descarga Kodosi](README.md#descargar-kodosi) para tu ordenador. Los atajos de esta guía usan las
teclas de macOS; en Linux y Windows, sustituye ⌘ por Ctrl+Mayús salvo que se indique lo contrario.

## 1. Inicia sesión

Abre Kodosi y haz clic en **Iniciar sesión**, en la parte inferior de la barra lateral. Kodosi
abrirá tu navegador para que inicies sesión. Desde un terminal, también puedes ejecutar
`kodosi auth login` para obtener un código de dispositivo que podrás introducir en cualquier
navegador.

Puedes usar terminales locales sin iniciar sesión. Inicia sesión para conectar tus otros
ordenadores, añadir amigos y usar proyectos.

## 2. Inicia un agente

Pulsa ⌘N (o haz clic en **Nuevo terminal**) para abrir el lanzador. Elige un agente y una carpeta;
si quieres, selecciona también una cuenta, un modelo y un nivel de esfuerzo, escribe un prompt
inicial y pulsa Retorno. Kodosi escribirá el comando de inicio en una shell nueva.

<p align="center"><img src="../media/launcher.webp" width="580" alt="El lanzador configurado con Claude Code, una carpeta y un prompt inicial."></p>

Si un agente aún no está instalado, el lanzador muestra su comando de instalación.

### Formas rápidas de abrir terminales

- Pulsa ⌘D (Ctrl+Mayús+O en Linux y Windows) para abrir una shell normal en la carpeta del terminal
  seleccionado.
- Pulsa ⌘T (Ctrl+Mayús+T) para abrir un terminal nuevo con el mismo agente, cuenta y ajustes que el
  terminal seleccionado.
- Haz clic en el botón `+` junto a una carpeta de la barra lateral para iniciar otro terminal con
  el agente que ya se ejecuta en esa carpeta, con la misma cuenta y los mismos ajustes (o una shell
  normal si no hay ninguno en marcha ahí).
- En un terminal inactivo situado en el prompt de la shell, haz clic en el botón de acciones (dos
  controles deslizantes) de la cabecera para iniciar cualquier agente instalado con tus últimos
  ajustes.

### Atajos de teclado

Los atajos siguen los predeterminados de Ghostty y se pueden personalizar en **Ajustes › Atajos**:

- De ⌘1 a ⌘9 (de Alt+1 a Alt+9 en Linux y Windows): cambia a un terminal por su posición.
- ⌥⌘ y teclas de flecha (Ctrl+Alt y teclas de flecha): muévete a un terminal contiguo.
- ⌘W: cierra el terminal seleccionado.
- ⌘↑ y ⌘↓: salta entre prompts de la shell en zsh y en versiones modernas de bash (no en el bash
  3.2 incluido en macOS).

### Permisos, verbosidad y modo rápido

- Por defecto, los agentes se inician sin restricciones, de modo que pueden editar archivos y
  ejecutar comandos sin pedir confirmación. Antigravity CLI es la excepción: se inicia en su propio
  modo automático, que sigue preguntando antes de acciones irreversibles. Para que un agente pida
  permiso antes, selecciona **Preguntar** dentro de **Más** (después del selector de modelo).
- En **Más** también puedes elegir la verbosidad de salida para Claude Code, Codex, Copilot CLI y
  Antigravity CLI (por defecto usan **Breve**). Como Copilot CLI solo lee la verbosidad desde sus
  ajustes guardados, Kodosi actualiza temporalmente la configuración de Copilot mientras Copilot
  CLI se ejecuta en modo Breve y la restablece al terminar.
- Claude Code y Codex admiten un modo rápido (respuestas más rápidas a un coste mayor), que se
  activa junto a los selectores de modelo y esfuerzo. El modo rápido solo se aplica a ese inicio y
  vuelve a desactivarse la próxima vez.

### Cuentas y cambios sobre la marcha

- Al añadir una cuenta nueva se le asigna una carpeta de configuración aislada, como
  `~/.claude-work`. Kodosi también detecta cuentas definidas mediante alias de la shell, como
  `alias cw='CLAUDE_CONFIG_DIR=~/.claude-work claude'`.
- Mientras un agente está en marcha, usa el botón de acciones (dos controles deslizantes) de la
  cabecera para cambiar su modelo, esfuerzo, modo rápido y, en los agentes compatibles, sus
  permisos sin reiniciarlo. Kodosi envía el comando con barra del propio agente en cuanto termina
  el turno actual. Ten en cuenta que Claude Code recuerda los cambios de modelo, esfuerzo y modo
  rápido también para sesiones futuras.

### Enlaces, imágenes, worktrees y temas

Mantén pulsada ⌘ y haz clic en un enlace (Ctrl-clic en Linux y Windows) para abrirlo en el
navegador predeterminado del dispositivo que estés usando; mantén pulsada también Mayús si el
programa en ejecución captura el ratón. En el iPhone, toca el enlace (esta acción queda desactivada
mientras un programa captura el ratón). Kodosi te pregunta antes de pasar un enlace a otra app y
bloquea la apertura de archivos ejecutables, así como de rutas de archivos locales o direcciones
`localhost` procedentes de ordenadores remotos.

Para compartir una captura de pantalla o una imagen con un agente, pégala con ⌘V (Ctrl+Mayús+V en
Linux y Windows, o Pegar en el iPhone). Kodosi la guarda en el ordenador anfitrión del terminal y
pega su ruta de archivo para que el agente pueda adjuntarla, mostrando el progreso de la subida en
terminales remotos. El atajo de pegado propio del agente (como Ctrl+V en Claude Code) sigue leyendo
el portapapeles del ordenador donde se ejecuta el agente.

Para darle a un agente una copia aislada de un repositorio de Git, pasa el cursor sobre la carpeta
del repositorio en la barra lateral, haz clic en el botón de rama y escribe un nombre de rama.
Kodosi abre el terminal en un worktree de Git nuevo y lo elimina automáticamente al cerrar el
terminal si no ha habido cambios. Puedes configurar dónde se guardan los worktrees en **Ajustes ›
Terminal**.

Cada terminal es claro u oscuro como Kodosi en su ordenador anfitrión, en todos los dispositivos
desde los que lo veas. Los programas dentro del terminal reciben notificaciones del tema, de modo
que herramientas como Claude Code (con `/theme` en Auto) se adaptan automáticamente. Si prefieres
terminales oscuros aunque Kodosi esté en modo claro, activa **Mantener los terminales oscuros** en
**Ajustes › Terminal**.

### Cerrar, salir y permisos de macOS

Cada terminal es una shell real que se ejecuta en su ordenador anfitrión. **Minimizar** oculta un
terminal manteniéndolo activo; **Cerrar** finaliza la shell y sus procesos hijos. Cerrar la ventana
de la app deja Kodosi y tus terminales funcionando en la barra de menús o en la bandeja del sistema
(salvo en escritorios de Linux sin bandeja del sistema, donde cerrar la ventana cierra Kodosi).
Puedes cambiar el comportamiento al cerrar la ventana en **Ajustes**.

Cuando sales de Kodosi, tus terminales se detienen y Kodosi guarda el estado de su pantalla de forma
local en tu ordenador (nunca en el servidor). La próxima vez que abras Kodosi, tus terminales se
vuelven a abrir en las mismas carpetas, proyectos y posiciones en el escenario, con su pantalla e
historial anteriores sobre un prompt nuevo (en Windows, desplázate hacia arriba para verlos).
Claude Code, Codex y Copilot CLI reanudan automáticamente sus conversaciones y, en macOS y Linux,
mantienen también el modelo, el esfuerzo, el modo rápido, los permisos y la verbosidad que elegiste.
Si Kodosi no puede identificar qué conversación estaba ejecutando un agente, el agente muestra la
lista de sus conversaciones para que elijas una.

En macOS, los programas dentro de Kodosi heredan los permisos de macOS concedidos a Kodosi, igual
que en cualquier otra app de terminal. Si macOS impide que un programa modifique otra app, activa
Kodosi en **Gestión de apps** (accesible desde **Ajustes › General**). Allí también puedes conceder
**Control de dispositivos y acceso a datos** (llamado **Accesibilidad** antes de macOS 27) y
**Grabación de pantalla y del audio del sistema** para herramientas que automatizan apps o capturan
la pantalla.

## 3. Mira qué agente te necesita

El icono de cada terminal muestra su estado actual:

| Indicador | Significado |
| --- | --- |
| Luz cálida que orbita alrededor del icono | Trabajando (un anillo que se va llenando muestra el porcentaje de progreso cuando el programa lo indica). |
| Mano, signo de interrogación o llave | Esperando tu aprobación, tu respuesta o un inicio de sesión. |
| Marca de verificación verde | Terminado. |
| Triángulo de advertencia amarillo | Ha fallado. |

La marca de verificación y la advertencia desaparecen en cuanto abres ese terminal en cualquier
dispositivo.

Cuando un terminal pasa a esperar, termina o falla, Kodosi te avisa una sola vez en el dispositivo
que estés usando en ese momento:

- Si Kodosi está en primer plano, simplemente ves el indicador de estado.
- Si estás usando otra app en un ordenador, Kodosi muestra una notificación de escritorio ahí.
- Si no estás delante de ninguno de tus ordenadores, Kodosi envía una notificación push a tu
  teléfono. Si un terminal en espera sigue sin abrirse cuando te alejas del ordenador, el aviso se
  reenvía entonces a tu teléfono.

Solo los terminales en espera reproducen sonido de notificación. Puedes silenciar las notificaciones
en cada dispositivo desde **Ajustes**. Los programas comunican su estado mediante el
[Program Status Protocol](https://www.superlogical.com/rex/docs/build/program-status) o las
secuencias estándar de progreso de terminal; los programas que no comunican su estado simplemente
no muestran ningún indicador.

### Vista de chat

Los terminales con Claude Code, así como los de Codex y Copilot CLI iniciados a través de Kodosi,
también ofrecen una vista **Chat** en la cabecera del terminal. El chat muestra tus mensajes, las
respuestas y la actividad de herramientas del agente, y cualquier pregunta pendiente con botones en
los que puedes hacer clic. Puedes responder o detener al agente directamente desde el chat; si
escribes un texto propio mientras hace una pregunta, le indica al agente qué hacer en su lugar.
Todo lo que haces en el chat ocurre también en el terminal subyacente.

## 4. Mira lo que muestra un agente en el lienzo

Los agentes pueden mostrar páginas HTML, Markdown, imágenes, PDF, documentos de Office, hojas de
cálculo, presentaciones o aplicaciones web en ejecución en un **lienzo** junto al terminal, y
controlar un navegador compartido. Por defecto, el lienzo se abre automáticamente cuando el agente
muestra o actualiza algo mientras estás viendo ese terminal (salvo que lo hayas cerrado). Para
abrirlo solo tú, desactiva **Abrir el lienzo automáticamente** en **Ajustes › General**.

Claude Code, Copilot CLI y Antigravity CLI saben usar el lienzo incluso cuando se inician
manualmente, y Codex, Cursor y OpenCode lo saben cuando los inicia Kodosi (OpenCode en macOS y
Linux). Con cualquier otro agente, dile que ejecute `kodosi skill canvas`.

En cuanto el lienzo tiene contenido, la cabecera del terminal añade los botones **Archivos** y
**Navegador**, con un punto cobrizo siempre que haya novedades:

- Haz clic en cualquiera de los dos botones para abrir el lienzo junto al terminal.
- Haz clic en **Más espacio** para que el lienzo ocupe el escenario mientras el terminal se reduce
  a una esquina.
- Cierra un panel con la `×` de su botón y vuelve a abrirlo con `+`.
- Abre el navegador manualmente desde **Abrir navegador** en el menú de acciones, o añade archivos
  mediante **Añadir un archivo…** (o arrastrándolos sobre el lienzo), lo que también copia el
  archivo a la carpeta del terminal.
- En el iPhone, el botón del lienzo está en la barra del terminal, y un aviso anuncia los elementos
  nuevos o actualizados con un botón **Abrir**.

### Documentos en directo y páginas

- Los documentos (`.docx`), hojas de cálculo (`.xlsx`), presentaciones (`.pptx`), PDF y archivos de
  texto funcionan en directo: tú, cualquier persona que comparta el terminal y el agente podéis
  editarlos a la vez mientras siguen siendo archivos normales en el disco. Cuando un documento
  contiene control de cambios, la barra de herramientas incluye un botón para aceptarlos o
  rechazarlos. En **Ajustes › Documentos** puedes activar o desactivar cada visor, alternar entre
  vista paginada y texto fluido, y activar páginas oscuras en modo oscuro.
- Las páginas HTML del lienzo se ejecutan completamente sin conexión y sin acceso a la red. Los
  enlaces en páginas, documentos, hojas de cálculo, presentaciones y PDF se abren en tu propio
  navegador con las mismas reglas de seguridad que los enlaces del terminal (⌘-clic o Ctrl-clic en
  vistas editables; clic normal en el resto; toque en el iPhone). Los enlaces internos a
  diapositivas o celdas saltan directamente a ese punto.
- Selecciona texto en una página o un documento y haz clic en **Preguntar sobre esto** para enviar
  el nombre del archivo y el texto seleccionado directamente al terminal y escribir allí tu
  pregunta.
- Haz clic derecho en la pestaña de un archivo (o mantenla pulsada en el móvil) para guardar una
  copia, quitar ese archivo o todos los archivos del lienzo, o mover el archivo a la papelera del
  ordenador anfitrión.

### Navegador compartido

- Haz clic y escribe dentro del navegador compartido igual que en un navegador normal. El agente ve
  las mismas páginas y sesiones abiertas, y la salida de su siguiente comando del navegador le
  indica que has usado el navegador.
- Tras 10 minutos de inactividad, el navegador pasa a reposo y vuelve a abrir su última página
  cuando tú o el agente lo volvéis a usar. La primera vez que abres un navegador compartido en un
  ordenador, Kodosi descarga Kodosi Browser automáticamente, por lo que tarda un momento.
- Para reutilizar tus sesiones de navegador existentes en terminales privados, haz clic en
  **Copiar** junto a un navegador en **Ajustes › Navegador**. Haz clic en **Quitar** para borrar lo
  que copiaste, o en **Quitar todo** para eliminar todas las sesiones y datos de sitios de tus
  terminales privados. En macOS, si el sistema restringe el acceso a los datos de un navegador
  (como Safari), haz clic en **Permitir…** junto a ese navegador para conceder Acceso total al
  disco.
- Cualquier persona con acceso a un terminal ve su lienzo y comparte su navegador. Cerrar el
  terminal limpia su lienzo y descarta las sesiones del navegador de los terminales compartidos.

## 5. Pregúntale a Dosi

Dosi es tu agente para coordinar a tus otros agentes. Haz clic en la cara de Dosi, en la parte
inferior de la barra lateral (o pulsa ⌘J). Dosi funciona con Claude Code o Codex, que puedes
configurar junto con su cuenta, modelo y esfuerzo en **Ajustes › Dosi**.

<p align="center">
  <picture>
    <source media="(prefers-reduced-motion: reduce)" srcset="../media/dosi-still.webp">
    <img src="../media/dosi.webp" width="900" alt="Cuando le preguntas qué necesita atención, Dosi revisa los terminales, informa de que checkout espera aprobación y pregunta si debe permitirla.">
  </picture>
</p>

Pregúntale a Dosi qué necesita tu atención o pídele que se encargue de algo. Puede revisar tus
terminales y proyectos, responder a solicitudes, iniciar o cerrar terminales y transmitir mensajes.
Cuando hace falta tomar una decisión, Dosi presenta botones con opciones junto a un cuadro para
responder con tus propias palabras.

Tu conversación con Dosi se mantiene sincronizada en todos tus dispositivos. En el iPhone y el iPad,
elige justo encima del cuadro de texto al Dosi de qué ordenador quieres escribir (por defecto, el
último que respondió). Los mensajes enviados a un ordenador sin conexión quedan en cola hasta que
vuelva a conectarse, y puedes cancelar un mensaje en cola con **No enviar**.

La línea de estado junto a la cara de Dosi muestra cuándo está trabajando, cuándo ha respondido o
cuándo espera respuesta a una pregunta. Si todavía no hay ningún agente instalado en ese ordenador,
muestra **Configurar Dosi** y abre **Ajustes › Dosi**. Para desactivar Dosi en un ordenador,
desactiva **Usar Dosi** en **Ajustes › Dosi**.

## 6. Añade tus otros ordenadores y teléfonos

Inicia sesión en un ordenador nuevo con la misma cuenta. Mostrará un código de verificación y tus
dispositivos actuales te pedirán aprobarlo. Haz clic en **Aprobar** en un ordenador ya aprobado e
introduce el código, o escanéalo desde un teléfono aprobado.

Añadir un teléfono o una tableta solo lleva un escaneo: en un ordenador ya aprobado, abre
**Ajustes**, haz clic en **Mostrar un código** en **Añadir un dispositivo** y escanéalo con el
dispositivo nuevo. Una vez aprobado, los terminales de todos tus ordenadores aparecerán en todas
partes.

### Iniciar terminales de forma remota

Para iniciar un terminal en otro ordenador, selecciónalo en el lado derecho de la barra del
lanzador. El menú de carpetas mostrará entonces las carpetas recientes de esa máquina, y **Elegir
una carpeta…** permite explorar su directorio personal. En el iPhone y el iPad, toca `+` en
**Terminales** o junto a un ordenador concreto. El ordenador de destino necesita tener Kodosi
abierto (o `kodosi host` ejecutándose en un terminal).

Cuando dos de tus dispositivos pueden alcanzarse directamente entre sí, el tráfico de terminales y
del navegador viaja de punto a punto para reducir la latencia. En una red local, macOS, iOS y
Android piden una vez permiso de acceso a la red local; si lo rechazas, el tráfico pasa por el
servidor de retransmisión.

### Claves de recuperación

Crea una clave de recuperación en **Ajustes** y guárdala en un lugar seguro. Si alguna vez pierdes
todos tus dispositivos, inicia sesión en uno nuevo y selecciona **«Usar mi clave de recuperación»**
para conservar tus amigos y proyectos. Quita los dispositivos perdidos desde cualquiera de los que
conserves; si te han robado un dispositivo, genera después una clave de recuperación nueva.

## 7. Trabaja con amigos

Abre **Amigos** desde el menú situado junto a tu cuenta. Envía tu invitación, o pega la
invitación o el nombre de usuario de un amigo. Usar una invitación permite a Kodosi verificar
automáticamente la identidad de tu amigo. En el móvil puedes mostrar tu invitación como un código QR
para que un amigo lo escanee con su cámara o desde Kodosi; aun así, tendrá que confirmar que confía
en ti antes de enviar la solicitud.

<p align="center"><img src="../media/friends.webp" width="900" alt="La página Amigos mostrando dos amigos verificados."></p>

### Compartir un terminal

Abre el menú de acciones en la cabecera de un terminal, haz clic en **Compartir** y selecciona
amigos o proyectos. Cualquier persona con la que compartas el terminal obtiene control total con
tus permisos del sistema operativo en esa máquina. Tus otros terminales siguen siendo privados.

### Proyectos

Un proyecto reúne terminales de cualquier carpeta u ordenador junto con un chat compartido y una
lista de tareas. Haz clic en `+` junto a **Proyectos**, inicia terminales dentro del proyecto (o
comparte en él terminales que ya tengas abiertos) e invita a tus amigos cuando estés listo.

<p align="center"><img src="../media/project.webp" width="900" alt="Un proyecto con un terminal compartido y una conversación entre personas y agentes."></p>

- Todos los miembros pueden usar los terminales compartidos y compartir los suyos. Quienes se unan
  más tarde ven de inmediato los terminales del proyecto y todo el historial del chat. Los archivos
  permanecen en el ordenador anfitrión de cada terminal.
- Crea tareas disponibles para cualquiera, reclámalas, libéralas, ciérralas con una nota resumen o
  vuelve a abrirlas.
- Vincula repositorios de GitHub o Gitea para sincronizar las issues con las tareas: al actualizar
  una tarea vinculada se actualiza la issue correspondiente. GitHub usa tu sesión de `gh` o el
  asistente de credenciales de Git; Gitea usa el asistente de credenciales de Git o
  `KODOSI_GITEA_TOKEN` junto con `KODOSI_GITEA_URL`. Las credenciales nunca salen de tu ordenador.
- Menciona un terminal en el chat con `@<terminal-name>` para entregar tu mensaje (etiquetado con
  tu nombre y el proyecto) directamente al agente que se ejecuta ahí; el agente decide cómo
  responder. Claude Code, Copilot CLI y Antigravity CLI dentro de Kodosi (además de Codex, Cursor y
  OpenCode cuando los inicia Kodosi, con OpenCode en macOS y Linux) saben automáticamente que están
  en Kodosi. Ejecuta `kodosi skill` para imprimir la [skill](../../runtime/skills/kodosi/SKILL.md)
  para cualquier otro agente. Dentro de un terminal de Kodosi, los comandos de proyecto detectan el
  proyecto actual automáticamente; fuera, pasa `--project <name>`.
- Eliminar un mensaje publicado por ti o por tu agente lo sustituye por un marcador de mensaje
  eliminado para todos. **Vaciar chat** oculta el historial hasta ese momento en todos tus
  dispositivos, dejándolo intacto para el resto de los miembros.

## 8. Usa la línea de comandos

La CLI `kodosi` controla exactamente los mismos terminales que la app de escritorio, y los cambios
aparecen en la app al instante.

| Comando | Qué hace |
| --- | --- |
| `kodosi terminal start --agent claude --prompt "fix the failing test"` | Inicia un agente con tus últimos ajustes. Puedes modificarlos con `--account`, `--model`, `--effort`, `--fast`, `--permissions` y `--output`. |
| `kodosi terminal start --directory <repository> --branch <name>` | Inicia un terminal en una rama nueva dentro de su propio worktree de Git. |
| `kodosi terminal start --computer <computer> --directory <folder>` | Inicia un terminal en otro de tus ordenadores. Las rutas relativas y `~` se resuelven respecto a tu directorio personal en esa máquina. Usa `kodosi devices list` para ver qué ordenadores están en línea. |
| `kodosi agent list` | Lista los agentes instalados, sus cuentas y tus últimos ajustes. |
| `kodosi agent usage` | Muestra el uso actual del plan y las horas de restablecimiento de las cuentas con sesión iniciada de Claude Code, Codex y Copilot CLI. |
| `kodosi terminal list` | Lista los terminales con sus nombres e ID (puedes pasar cualquiera de los dos como `<terminal>`). |
| `kodosi terminal read <terminal>` | Imprime el contenido actual de la pantalla. Añade `--history` para incluir el historial de desplazamiento. |
| `kodosi terminal message <terminal> "Can you review my branch?"` | Envía un mensaje al agente de un terminal, etiquetado con el remitente y el origen. |
| `kodosi terminal agent <terminal> --model sonnet` | Cambia el modelo, el esfuerzo, el modo rápido o los permisos de un agente en ejecución. |
| `kodosi terminal input <terminal> --enter "make test"` | Escribe texto en un terminal. |
| `kodosi terminal keys <terminal> ctrl-c` | Envía pulsaciones de teclas a un terminal. |
| `kodosi terminal wait <terminal> --until done,failed` | Espera hasta que el terminal entre en uno de los estados indicados. |
| `kodosi terminal show <terminal>` | Enfoca el terminal en la app. |
| `kodosi dosi say "What needs me?"` | Envía un mensaje a Dosi. Usa `kodosi dosi show` para imprimir la conversación y `kodosi dosi stop` para cancelar un turno. |
| `kodosi show <file>` | Abre un archivo en el lienzo de este terminal, que se abre para las personas que están viendo el terminal. Usa `kodosi show <address>` para abrir una URL en su navegador compartido, `kodosi show browser` para abrir el navegador tal cual, `kodosi canvas` para listar los elementos del lienzo y `kodosi canvas remove <file>` para quitar uno. |
| `kodosi browser open <address>` | Abre una URL en el navegador compartido de este terminal (el resto de los comandos de agent-browser también funcionan). |
| `kodosi skill` | Imprime la skill de agente para colaborar en Kodosi; `kodosi skill canvas` imprime la skill del lienzo. |
| `kodosi dosi skill` | Imprime la skill que permite a otro agente actuar como Dosi. |
| `kodosi host` | Ejecuta el host de forma independiente para que los terminales sigan funcionando tras cerrar la app. Usa `kodosi host stop` para detener un host en segundo plano. |

`message`, `input` y `keys` no se ejecutan mientras estés escribiendo activamente en ese terminal.
`message` tampoco se ejecuta mientras el agente espera la respuesta de una persona.

## Si algo no conecta

- Comprueba que el ordenador anfitrión del terminal está en línea con Kodosi en ejecución y que el
  terminal sigue compartido.
- Un dispositivo recién añadido puede estar aún pendiente de aprobación, o un amigo cuya identidad
  haya cambiado puede necesitar una nueva verificación en **Amigos**.
- Después de perder la conexión, Kodosi vuelve a mostrar el estado actual. Si un terminal remoto
  muestra **Sin conexión**, este dispositivo no puede llegar al servidor; comprueba tu conexión de
  red. Kodosi se reconecta automáticamente, o haz clic en **Reintentar** para intentarlo de
  inmediato.
- Cuando un ordenador anfitrión se desconecta, sus terminales se reconectan automáticamente en
  cuanto vuelve a estar en línea (salvo que se hayan cerrado mientras tanto). Para quitar un
  terminal sin conexión de tu lista, haz clic en su `×`: **Ocultar hasta que vuelva** lo oculta
  temporalmente, y **Cerrar para siempre** lo cierra de forma definitiva cuando el ordenador vuelva
  a conectarse.
- Si Kodosi avisa de que la entrega de tu entrada ha sido incierta, revisa la pantalla del terminal
  antes de volver a ejecutar el comando.
