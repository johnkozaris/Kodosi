<p align="center">
  <a href="https://kodosi.com"><img src="../media/banner.webp" width="800" alt="El logotipo de Kodosi sobre las palabras «terminals, agents and friends. together.» y un cursor de terminal."></a>
</p>

<p align="center">
  <a href="../../README.md">English</a> · <a href="../de/README.md">Deutsch</a> · <a href="../el/README.md">Ελληνικά</a> · <b>Español</b> · <a href="../fr/README.md">Français</a> · <a href="../it/README.md">Italiano</a> · <a href="../ja/README.md">日本語</a> · <a href="../ko/README.md">한국어</a> · <a href="../zh-Hans/README.md">简体中文</a>
</p>

<p align="center">
  <a href="../../LICENSE"><img src="https://img.shields.io/badge/license-Apache_2.0-c98566?style=flat-square&labelColor=2a211d" alt="Licencia Apache 2.0"></a>
  <a href="#descargar-kodosi"><img src="https://img.shields.io/badge/macOS-26.4%2B%20%C2%B7%20Apple%20silicon-c98566?style=flat-square&labelColor=2a211d" alt="macOS 26.4 o posterior en Apple silicon"></a>
  <a href="#descargar-kodosi"><img src="https://img.shields.io/badge/Linux-x86--64-c98566?style=flat-square&labelColor=2a211d" alt="Linux x86-64"></a>
  <a href="#descargar-kodosi"><img src="https://img.shields.io/badge/Windows-11%2024H2%2B%20%C2%B7%20x64-c98566?style=flat-square&labelColor=2a211d" alt="Windows 11 24H2 o posterior en x64"></a>
  <a href="../../clients/ios/README.md"><img src="https://img.shields.io/badge/iOS-26%2B%20%C2%B7%20iPhone%20and%20iPad-c98566?style=flat-square&labelColor=2a211d" alt="iOS 26 o posterior en iPhone y iPad"></a>
  <a href="https://github.com/johnkozaris/Kodosi/releases/latest"><img src="https://img.shields.io/github/v/release/johnkozaris/Kodosi?style=flat-square&labelColor=2a211d&color=c98566&label=release" alt="Última versión"></a>
</p>

[Kodosi](https://kodosi.com) es una app de terminal para agentes de programación en Mac,
Linux y Windows. Reúne los terminales de todos tus ordenadores en una ventana, te muestra qué
agente te necesita y deja que tus amigos trabajen en los mismos terminales. Los terminales y
las salas están cifrados de extremo a extremo.

<p align="center">
  <picture>
    <source media="(prefers-color-scheme: light)" srcset="../media/app-light.webp">
    <img src="../media/app-dark.webp" width="900" alt="La app de Kodosi para Mac: Claude Code espera una aprobación mientras Codex trabaja.">
  </picture>
</p>

## Qué hace Kodosi

**Todos los terminales en una ventana.** Los terminales de tu Mac, de tu ordenador con Linux o
Windows y de los ordenadores de tus amigos están en una sola barra lateral. Cada uno es una
shell real en su propio ordenador, y desde la misma ventana inicias uno nuevo en cualquiera de
tus ordenadores.

**Sabe qué agente te necesita.** Usa Claude Code, Codex, Copilot CLI o cualquier otro agente
de terminal. Cada terminal muestra cuándo su agente trabaja, espera tu aprobación, ha
terminado o ha fallado, y una notificación te avisa mientras Kodosi está en segundo plano.

**Dosi, tu agente para tus agentes.** Pregúntale a Dosi qué te necesita. Lee todos tus
terminales y salas, te lo cuenta y actúa por ti: escribe la respuesta en un terminal, inicia
agentes y cierra lo que ya está hecho. Tu conversación con Dosi te sigue a cualquier
ordenador.

<p align="center">
  <picture>
    <source media="(prefers-reduced-motion: reduce)" srcset="../media/dosi-still.webp">
    <img src="../media/dosi.webp" width="900" alt="Cuando le preguntas qué te necesita, Dosi lee los terminales, dice que checkout espera una aprobación y pregunta si debe permitirla.">
  </picture>
</p>

**Comparte un terminal en directo.** Dale a un amigo el control total de un terminal en marcha
y de su agente.

**Un lienzo junto a cada terminal.** Un agente te muestra una página, un plan o una imagen
junto a su terminal. Los documentos, las hojas, las presentaciones, los PDF y los archivos
de texto están en directo: tú, tus amigos y el agente editáis el mismo archivo real a la
vez, y tú aceptas o rechazas los cambios del agente. El agente también usa un navegador que
compartes con él: ves lo que hace como vídeo en directo, y haces clic y escribes en las
mismas páginas.

<p align="center">
  <picture>
    <source media="(prefers-reduced-motion: reduce)" srcset="../media/canvas-still.webp">
    <img src="../media/canvas.webp" width="900" alt="El terminal de Claude Code con su lienzo: un plan de lanzamiento que Leo edita mientras Claude sugiere un cambio, luego una hoja de presupuesto, diapositivas, un PDF y un navegador compartido donde Claude añade una taza al carrito.">
  </picture>
</p>

**Salas para personas y agentes.** Una sala tiene terminales compartidos, una conversación y
tareas. Los agentes leen y escriben en la conversación y se encargan de tareas, y las issues
de GitHub o Gitea se convierten en tareas.

<p align="center"><img src="../media/room.webp" width="900" alt="Una sala con un terminal compartido y una conversación de personas y agentes."></p>

**Una rama para cada agente.** Inicia un agente en una rama nueva, en su propio worktree de
Git, para que dos agentes nunca cambien los mismos archivos.

**En tu iPhone y tu iPad.** Sigue, responde y controla tus terminales y salas cuando no estás
en tu ordenador.

**Cifrado de extremo a extremo y poscuántico.** Tus claves se quedan en tus dispositivos. Tus
dispositivos cifran los terminales y las salas de extremo a extremo con ML-KEM-768 junto con
X25519, así que el servicio no puede leerlos. [Seguridad](SECURITY.md)

## Descargar Kodosi

<p>
  <a href="https://updates.kodosi.com/download/macos"><img src="https://img.shields.io/badge/Descargar-Mac-c98566?style=for-the-badge&labelColor=2a211d" alt="Descargar Kodosi para Mac"></a>
  <a href="https://updates.kodosi.com/download/linux"><img src="https://img.shields.io/badge/Descargar-Linux-c98566?style=for-the-badge&labelColor=2a211d" alt="Descargar Kodosi para Linux"></a>
  <a href="https://updates.kodosi.com/download/windows"><img src="https://img.shields.io/badge/Descargar-Windows-c98566?style=for-the-badge&labelColor=2a211d" alt="Descargar Kodosi para Windows"></a>
</p>

- **Mac** (Apple silicon, macOS 26.4 o posterior): abre la imagen de disco y mueve Kodosi a
  Aplicaciones, o ejecuta `brew install --cask johnkozaris/tap/kodosi`. La app se actualiza
  sola.
- **Linux** (x86-64, Ubuntu 24.04 o Debian 13 o posterior): instala el paquete Debian. Añade
  el repositorio apt de Kodosi, así que las actualizaciones del sistema también actualizan
  Kodosi. La [versión](https://github.com/johnkozaris/Kodosi/releases/latest) también tiene
  un paquete para Arch y un archivo portable.
- **Windows** (x64, Windows 11 24H2 o posterior): antes de abrir la descarga por primera vez,
  añade como administrador el [certificado de Kodosi](https://updates.kodosi.com/windows/Kodosi.cer)
  a Equipo local › Personas de confianza. La app se actualiza sola.
- **iPhone y iPad:** compila Kodosi para tu [iPhone o iPad](../../clients/ios/README.md).

Después sigue los [primeros pasos](GETTING_STARTED.md).

## Más información

- [Primeros pasos](GETTING_STARTED.md)
- [Seguridad](SECURITY.md) y el [diseño del cifrado](../CRYPTOGRAPHY.md) (en inglés)
- [Arquitectura](../ARCHITECTURE.md), [desarrollo](../DEVELOPMENT.md) y
  [cómo contribuir](../../CONTRIBUTING.md) (en inglés)
- [Alojamiento propio](../SELF-HOSTING.md) (en inglés)

Informa de bugs en las [issues de GitHub](https://github.com/johnkozaris/Kodosi/issues) y de
vulnerabilidades [en privado](SECURITY.md#informar-de-una-vulnerabilidad).

Kodosi lo crea [John Kozaris](https://johnkozaris.com).

## Licencia

Kodosi se distribuye con la [licencia Apache 2.0](../../LICENSE). El nombre y el logotipo de
Kodosi no están bajo esa licencia; las [marcas registradas](../../TRADEMARKS.md) (en inglés)
explican cómo puedes usarlos. Kodosi usa [Ghostty](https://ghostty.org), Rust, Swift, Qt y
ASP.NET Core. Los [avisos de terceros](../../terminal/ghostty/THIRD_PARTY_NOTICES.md) y los
[créditos de las imágenes](../media/README.md) indican lo que incluye.
