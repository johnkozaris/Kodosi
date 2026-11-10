# Primeros pasos

[English](../GETTING_STARTED.md) · [Deutsch](../de/GETTING_STARTED.md) · [Ελληνικά](../el/GETTING_STARTED.md) · **Español** · [Français](../fr/GETTING_STARTED.md) · [Italiano](../it/GETTING_STARTED.md) · [日本語](../ja/GETTING_STARTED.md) · [한국어](../ko/GETTING_STARTED.md) · [简体中文](../zh-Hans/GETTING_STARTED.md)

[Descarga Kodosi](README.md#descargar-kodosi) para tu ordenador. Esta guía usa las teclas del
Mac; en Linux y Windows, usa Ctrl+Mayús con la misma letra.

## 1. Inicia sesión

Abre Kodosi y selecciona Iniciar sesión en la parte inferior de la barra lateral. Kodosi abre
tu navegador, donde inicias sesión. En un terminal, `kodosi auth login` muestra un código para
escribirlo en cualquier navegador.

Sin cuenta, Kodosi ejecuta terminales en este ordenador. Inicia sesión para usar tus otros
ordenadores, tus amigos y tus proyectos.

## 2. Inicia un agente

Selecciona Nuevo terminal (⌘N). Elige un agente y una carpeta y, si quieres, una cuenta, un
modelo y un esfuerzo. Escribe un primer prompt y pulsa Retorno. Kodosi escribe el comando en
la shell del terminal nuevo.

<p align="center"><img src="../media/launcher.webp" width="580" alt="Claude Code elegido en el lanzador, con una carpeta y un primer prompt."></p>

- Para un terminal sin más, pulsa ⌘T (Ctrl+Mayús+T en Linux y Windows). Se abre al momento en
  la carpeta del terminal seleccionado.
- Las teclas siguen a Ghostty. ⌘1 a ⌘9 seleccionan un terminal (Alt+1 a Alt+9 en Linux y
  Windows), ⌥⌘ con una flecha selecciona el de al lado (Ctrl+Alt con una flecha) y ⌘W cierra
  el terminal seleccionado. Ajustes › Atajos muestra todas las teclas y las cambia.
- Si falta un agente, el lanzador muestra su comando de instalación.
- Los agentes empiezan con Acceso total: cambian archivos y ejecutan comandos sin
  preguntarte. Para que un agente pregunte antes, elige Preguntar en el botón que sigue al
  modelo. Ahí también eliges cuánto de su trabajo muestran Claude Code y Codex; empiezan en
  Breve.
- Una cuenta nueva tiene su propia carpeta de inicio de sesión, como `~/.claude-work`. Kodosi
  también encuentra las cuentas que configura tu shell, como
  `alias cw='CLAUDE_CONFIG_DIR=~/.claude-work claude'`.
- En un terminal que está en su prompt, el botón de acciones (dos reguladores) de su cabecera
  muestra los agentes instalados. Selecciona uno para iniciarlo con tu última elección.
- Mientras un agente trabaja, el mismo botón de acciones cambia su modelo y su esfuerzo y, en
  algunos agentes, sus permisos, con los comandos del propio agente, sin volver a iniciarlo.
  Un cambio espera a que el agente termine su turno, y Claude Code también conserva el modelo
  y el esfuerzo para sus próximas sesiones.
- En zsh y bash (no en el bash 3.2 de macOS), ⌘↑ y ⌘↓ saltan entre prompts.
- Haz ⌘-clic en un enlace para abrirlo (Ctrl-clic en Linux y Windows). En un programa que usa
  el ratón, mantén pulsada también Mayús. En el iPhone, toca el enlace; un toque no abre un
  enlace en un programa que usa el ratón. El enlace se abre en tu propio navegador, en el
  dispositivo que estás usando. Kodosi pregunta antes de abrir un enlace en otra app. No abre
  un archivo que pueda ejecutar un programa, ni un archivo o una dirección `localhost` de un
  terminal en otro ordenador.
- Para darle una imagen a un agente, como una captura de pantalla, pégala con ⌘V
  (Ctrl+Mayús+V en Linux y Windows, Pegar en el iPhone). Kodosi la guarda en el ordenador del
  terminal y pega la ruta de su archivo, y el agente la adjunta. Un terminal en otro
  ordenador muestra cuánto de la imagen ha llegado. La tecla propia del agente para imágenes,
  como Ctrl+V en Claude Code, lee el portapapeles del ordenador donde se ejecuta el agente.
- Para darle a un agente su propia copia de un repositorio, pasa el puntero por la carpeta
  del repositorio en la barra lateral, selecciona el botón de rama y ponle nombre a una rama.
  Kodosi inicia el terminal en un worktree de Git nuevo. Cuando el terminal termina, Kodosi
  quita el worktree si no ha cambiado nada. Ajustes › Terminal define dónde van esas
  carpetas.

Un terminal es una shell real en el ordenador que lo ejecuta. Minimizar oculta un terminal y
lo deja en marcha; Cerrar lo termina junto con sus programas. Cuando cierras la ventana, Kodosi
y tus terminales siguen en marcha, y el icono de Kodosi se queda en la barra de menús o en la
bandeja del sistema; en un escritorio sin bandeja del sistema, cerrar la ventana cierra
Kodosi. Cuando sales de Kodosi, tus terminales se detienen. Kodosi guarda lo que muestran en
tu ordenador y en ningún otro sitio. Cuando vuelves a abrir Kodosi, se inician de nuevo en sus
carpetas, proyectos y posiciones en el escenario, con su pantalla y su historial anteriores sobre
un prompt nuevo (en Windows, desplázate hacia arriba para verlos). Claude Code, Codex y Copilot
CLI continúan sus conversaciones, y en un Mac o en Linux mantienen el modelo, el esfuerzo, los
permisos y la salida que elegiste. Cuando Kodosi no puede saber qué conversación mostraba un
agente, el agente muestra sus conversaciones para que continúes una. En Ajustes eliges qué
hace cerrar la ventana.

En un Mac, los programas de tus terminales tienen los permisos que le das a Kodosi, como en
cualquier app de terminal. Cuando macOS dice que impidió que Kodosi modificara apps, un
programa de un terminal intentó cambiar otra app. Para permitirlo, activa Kodosi en Gestión
de apps; Ajustes › General la abre. Allí también permites el Control de dispositivos y
acceso a datos (Accesibilidad antes de macOS 27) y la Grabación de pantalla y del audio del
sistema, para los programas que controlan apps o ven la pantalla.

## 3. Mira qué agente te necesita

El icono de cada terminal muestra lo que hace su programa.

| Ves | Significa |
| --- | --- |
| Una luz cálida que gira alrededor del icono | Trabajando. El borde se llena para mostrar un porcentaje. |
| Una mano, un signo de interrogación o una llave | Espera tu aprobación, tu respuesta o tu inicio de sesión. |
| Una marca de verificación verde | Terminado. |
| Una señal de advertencia amarilla | Ha fallado. |

La marca de verificación y la advertencia desaparecen cuando abres el terminal. Mientras
Kodosi está en segundo plano, una notificación te avisa cuando uno de tus terminales empieza a
esperar, termina o falla. Los programas indican su estado con el
[Program Status Protocol](https://www.superlogical.com/rex/docs/build/program-status) o con
una barra de progreso. Un programa que no indica nada no muestra estado.

## 4. Mira lo que muestra un agente

Un agente puede mostrarte una página, Markdown, una imagen, un PDF, un documento, una hoja,
una presentación o la app que ejecuta en el lienzo de su terminal, y usar un navegador que
compartes con él. El lienzo se abre con lo que muestra el agente mientras miras su terminal.
Claude Code y Copilot CLI saben cómo, también cuando los inicias tú, y también un Codex que
inicia Kodosi; a otro agente, dile que lea `kodosi skill canvas`. Cuando el lienzo tiene algo,
la cabecera del terminal muestra un botón para sus archivos y otro para su navegador, con un
punto cobrizo cuando hay novedades. Selecciona uno para abrir el lienzo junto al terminal; Más
espacio le da el escenario al lienzo y deja el terminal pequeño en su esquina. Cierra una
parte con la × de su botón y vuelve a abrirla con +. Abrir navegador, en el botón de acciones,
abre el navegador antes de que lo use el agente, y Añadir un archivo… pone un archivo en el
lienzo y en la carpeta del terminal; también puedes soltar un archivo sobre el lienzo. En el
iPhone, el botón del lienzo está en la barra del terminal.

- Un documento, una hoja, una presentación, un PDF o un archivo de texto está en directo:
  tú, las personas del terminal y el agente lo editáis a la vez, y sigue siendo un archivo
  normal en su carpeta. Cuando un documento tiene cambios registrados, su barra tiene un
  botón que los acepta o los rechaza. Ajustes › Documentos activa o desactiva cada visor,
  muestra los documentos como páginas o como texto que se ajusta al lienzo, y oscurece las
  páginas cuando Kodosi está en modo oscuro.
- Una página no tiene red. Los enlaces de páginas, documentos, hojas, presentaciones y PDF se
  abren en tu propio navegador con las mismas reglas que los enlaces del terminal. Donde
  puedes editar, haz ⌘-clic en un enlace (Ctrl-clic en Linux y Windows); en el resto, basta
  un clic, y en el iPhone un toque. Un enlace a una diapositiva o a una celda del mismo
  archivo te lleva allí.
- Selecciona palabras en una página o en un documento y luego Preguntar sobre esto. Kodosi
  escribe el archivo y las palabras en el terminal, y tú escribes la pregunta.
- Haz clic y escribe en el navegador compartido como en el tuyo. El agente ve las mismas
  páginas y sesiones, y su siguiente comando le dice que has usado el navegador. Tras 10
  minutos sin uso, el navegador se duerme, y vuelve a abrir su última página cuando lo miras o
  cuando el agente lo usa. El primer navegador de un ordenador descarga Kodosi Browser, así
  que tarda un momento.
- Para usar tus sesiones en tus terminales privados, selecciona Copiar junto a un navegador
  en Ajustes › Navegador. Quitar elimina lo que copiaste, y Quitar todo elimina todas las
  sesiones y todos los datos de sitios de tus terminales privados. En un Mac, cuando macOS
  protege de Kodosi los inicios de sesión de un navegador, como los de Safari, selecciona
  Permitir… junto al navegador para dar a Kodosi Acceso total al disco.
- En una misma red, la imagen del navegador va directamente entre tus dispositivos. macOS e
  iOS piden una vez acceso a la red local; sin él, la imagen pasa por el servicio.
- Todas las personas que pueden abrir el terminal ven su lienzo y usan su navegador. Cuando
  el terminal se cierra, su lienzo y las sesiones del navegador desaparecen.

## 5. Pregúntale a Dosi

Dosi es tu agente para tus otros agentes. Selecciona la cara de Dosi en la parte inferior de
la barra lateral (⌘J). Dosi funciona con Claude Code o Codex; Ajustes › Dosi elige cuál, con
su cuenta, su modelo y su esfuerzo.

<p align="center">
  <picture>
    <source media="(prefers-reduced-motion: reduce)" srcset="../media/dosi-still.webp">
    <img src="../media/dosi.webp" width="900" alt="Cuando le preguntas qué te necesita, Dosi lee los terminales, dice que checkout espera una aprobación y pregunta si debe permitirla.">
  </picture>
</p>

Pregúntale a Dosi qué te necesita, o pídele que haga algo. Dosi lee tus terminales y proyectos,
responde, y escribe, inicia y cierra cosas por ti. Cuando necesita que elijas, pregunta con
botones, y también puedes responder con tus propias palabras. Tu conversación con Dosi es la
misma en todos tus dispositivos. En un iPhone o iPad, la pestaña Dosi escribe a Dosi en el
ordenador que eliges encima del mensaje, al principio el que respondió por última vez. Un
mensaje para un ordenador sin conexión espera hasta que vuelva a ejecutar Kodosi, y No enviar
lo retira.

La línea junto a la cara de Dosi te dice cuándo trabaja, cuándo ha respondido y cuándo tiene
una pregunta. En un ordenador sin agente, dice Configurar Dosi y abre Ajustes › Dosi. Para
quitar Dosi de un ordenador, desactiva Usar Dosi en Ajustes › Dosi.

## 6. Añade tus otros ordenadores

Inicia sesión en el ordenador nuevo con la misma cuenta. Muestra un código, y los ordenadores
que ya usas te piden que lo apruebes. Elige Aprobar en uno de ellos y escribe el código. Un
iPhone o un iPad se une de la misma manera. Después, cada ordenador muestra los terminales de
todos tus ordenadores.

Para iniciar un terminal en otro de tus ordenadores, elige ese ordenador en el extremo
derecho de la banda del lanzador. El botón de carpeta muestra entonces sus carpetas
recientes, y Elegir una carpeta… abre las carpetas de su directorio personal. En un iPhone o
iPad, selecciona + en Terminales, o el + junto a uno de tus ordenadores. Ese ordenador
necesita Kodosi abierto, o `kodosi host` ejecutándose en un terminal.

Crea también allí una clave de recuperación y guárdala bien. Si pierdes todos tus
dispositivos, inicia sesión en uno nuevo y elige «Usar mi clave de recuperación» para
conservar tus amigos y proyectos. Quita un dispositivo perdido desde otro dispositivo. Si te
robaron un dispositivo, crea también una clave de recuperación nueva.

## 7. Trabaja con amigos

Abre Amigos desde el menú junto a tu cuenta. Envía tu invitación, o pega la invitación o el
nombre de usuario de un amigo. Con una invitación, Kodosi puede verificar a tu amigo.

<p align="center"><img src="../media/friends.webp" width="900" alt="La página Amigos con dos amigos verificados."></p>

Para compartir un terminal, selecciona Compartir en el botón de acciones de su cabecera y
elige amigos o proyectos. Las personas con las que compartes tienen el control total, con tus
privilegios en ese ordenador. Tus otros terminales siguen siendo privados.

Un proyecto reúne terminales de cualquier carpeta y ordenador, con una conversación y
tareas. Selecciona + junto a Proyectos, luego inicia un terminal en el proyecto o comparte uno
en él, e invita a amigos cuando quieras.

<p align="center"><img src="../media/room.webp" width="900" alt="Un proyecto con un terminal compartido y una conversación de personas y agentes."></p>

- Todos los miembros pueden usar los terminales compartidos y compartir los suyos. Quien
  entra más tarde recibe los terminales y toda la conversación. Los archivos se quedan en el
  ordenador de cada terminal.
- Deja tareas disponibles, encárgate de ellas, suéltalas, ciérralas con un resultado o
  vuelve a abrirlas.
- Vincula repositorios de GitHub o Gitea para convertir las issues en tareas; cuando cambias
  una de esas tareas, Kodosi cambia su issue. GitHub usa tu sesión de `gh` o el credential
  helper de Git, y Gitea el credential helper o `KODOSI_GITEA_TOKEN` para el servidor de
  `KODOSI_GITEA_URL`. Tus credenciales se quedan en tu ordenador.
- Escribe `@` y el nombre de un terminal en el chat, y Kodosi le da al agente de ese terminal
  tu mensaje con tu nombre y el proyecto; el agente decide qué hacer con él. Claude Code y Copilot
  CLI en un terminal de Kodosi, y un Codex que inicia Kodosi, saben que se ejecutan en Kodosi,
  y `kodosi skill` imprime la [skill](../../runtime/skills/kodosi/SKILL.md) que permite a un
  agente participar. En un terminal de Kodosi, el comando encuentra el proyecto; en otro sitio,
  añade `--project <name>`.

## 8. Usa la línea de comandos

El comando `kodosi` trabaja con los mismos terminales que la app, y la app muestra lo que hace.

| Comando | Qué hace |
| --- | --- |
| `kodosi terminal start --agent claude --prompt "fix the failing test"` | Inicia un agente con tu última elección. `--account`, `--model`, `--effort`, `--permissions` y `--output` la cambian. |
| `kodosi terminal start --directory <repository> --branch <name>` | Inicia un terminal en una rama nueva, en su propio worktree. |
| `kodosi terminal start --computer <computer> --directory <folder>` | Inicia un terminal en otro de tus ordenadores. Una carpeta relativa, o una que empieza por `~`, está en tu directorio personal de allí. `kodosi devices list` muestra qué ordenadores están en línea. |
| `kodosi agent list` | Muestra los agentes, sus cuentas y tu última elección. |
| `kodosi agent usage` | Muestra cuánto de los límites del plan ha usado cada cuenta con sesión iniciada de Claude Code, Codex y Copilot CLI. |
| `kodosi terminal list` | Muestra tus terminales con sus nombres e ID. Indica cualquiera de los dos como `<terminal>`. |
| `kodosi terminal read <terminal>` | Imprime la pantalla. `--history` añade las líneas que se desplazaron fuera. |
| `kodosi terminal message <terminal> "Can you review my branch?"` | Le da un mensaje al agente de un terminal, con quién lo envía y desde dónde. |
| `kodosi terminal agent <terminal> --model sonnet` | Cambia el modelo, el esfuerzo o los permisos del agente de un terminal mientras trabaja. |
| `kodosi terminal input <terminal> --enter "make test"` | Escribe una línea. |
| `kodosi terminal keys <terminal> ctrl-c` | Pulsa teclas. |
| `kodosi terminal wait <terminal> --until done,failed` | Espera a un estado. |
| `kodosi terminal show <terminal>` | Muestra el terminal en la app. |
| `kodosi dosi say "What needs me?"` | Le pregunta a Dosi. `kodosi dosi show` imprime la conversación, y `kodosi dosi stop` detiene a Dosi. |
| `kodosi show <file>` | Muestra un archivo en el lienzo de este terminal, que se abre para quienes lo miran; `kodosi show <address>` muestra una dirección web en su navegador. `kodosi canvas` lista lo que muestra, y `kodosi canvas remove <file>` quita un archivo. |
| `kodosi browser open <address>` | Abre una página en el navegador compartido de este terminal. Los demás comandos de agent-browser también funcionan. |
| `kodosi skill` | Imprime la skill que permite a un agente trabajar con las personas y los agentes de Kodosi. `kodosi skill canvas` imprime la skill del lienzo. |
| `kodosi dosi skill` | Imprime la skill que permite a otro agente actuar como Dosi. |
| `kodosi host` | Mantiene tus terminales en marcha después de cerrar la app. `kodosi host stop` detiene un host que se ejecuta en segundo plano. |

`message`, `input` y `keys` se detienen con un mensaje mientras escribes en ese terminal.
`message` también se detiene mientras el agente espera la respuesta de una persona.

## Si algo no conecta

Comprueba que el ordenador del terminal está en línea, con Kodosi abierto, y que el terminal
sigue compartido. Un dispositivo nuevo puede necesitar aprobación, y un amigo con la identidad
cambiada puede necesitar verificación en Amigos. Después de perder la conexión, Kodosi vuelve
a mostrar el estado actual. Cuando un terminal de otro ordenador dice Sin conexión, este
dispositivo no llega al servidor: comprueba su red. Kodosi se vuelve a conectar solo, y
Reintentar conecta al momento. Cuando Kodosi dice que un ordenador está sin conexión, sus
terminales se vuelven a abrir cuando vuelve, salvo que hayan terminado allí. Para quitar un
terminal así de tu lista, selecciona su X. Ocultar hasta que vuelva lo muestra de nuevo cuando
vuelve el ordenador, y Cerrar para siempre lo cierra entonces. Si Kodosi dice que no está
seguro de que tu entrada haya llegado, mira el terminal antes de volver a escribir el comando.
