<p align="center">
  <a href="https://kodosi.com"><img src="../media/banner.webp" width="800" alt="El logotipo de Kodosi sobre las palabras «terminals, agents and friends. together.» y un cursor de terminal."></a>
</p>

<h3 align="center">Una app de terminal para ti, tus agentes de programación y tus amigos.</h3>

<p align="center">
  Mira qué agente te necesita en todos tus ordenadores, mira lo que crea junto a su terminal<br>
  y trabaja en los mismos terminales con tus amigos. Cifrado de extremo a extremo y de código abierto.
</p>

<p align="center">
  <a href="https://updates.kodosi.com/download/macos"><img src="https://img.shields.io/badge/Descargar-Mac-c98566?style=for-the-badge&labelColor=2a211d" alt="Descargar Kodosi para Mac"></a>
  <a href="https://updates.kodosi.com/download/linux"><img src="https://img.shields.io/badge/Descargar-Linux-c98566?style=for-the-badge&labelColor=2a211d" alt="Descargar Kodosi para Linux"></a>
  <a href="https://updates.kodosi.com/download/windows"><img src="https://img.shields.io/badge/Descargar-Windows-c98566?style=for-the-badge&labelColor=2a211d" alt="Descargar Kodosi para Windows"></a>
  <a href="https://updates.kodosi.com/download/android"><img src="https://img.shields.io/badge/Descargar-Android-c98566?style=for-the-badge&labelColor=2a211d" alt="Descargar Kodosi para Android"></a>
</p>

<p align="center">
  <a href="https://github.com/johnkozaris/Kodosi/releases/latest"><img src="https://img.shields.io/github/v/release/johnkozaris/Kodosi?style=flat-square&labelColor=2a211d&color=c98566&label=release" alt="Última versión"></a>
  <a href="../../LICENSE"><img src="https://img.shields.io/badge/license-Apache_2.0-c98566?style=flat-square&labelColor=2a211d" alt="Licencia Apache 2.0"></a>
</p>

<p align="center">
  <sub><a href="../../README.md">English</a> · <a href="../de/README.md">Deutsch</a> · <a href="../el/README.md">Ελληνικά</a> · <b>Español</b> · <a href="../fr/README.md">Français</a> · <a href="../it/README.md">Italiano</a> · <a href="../ja/README.md">日本語</a> · <a href="../ko/README.md">한국어</a> · <a href="../zh-Hans/README.md">简体中文</a></sub>
</p>

<p align="center">
  <picture>
    <source media="(prefers-color-scheme: light)" srcset="../media/app-light.webp">
    <img src="../media/app-dark.webp" width="900" alt="La app de Kodosi para Mac: Claude Code espera una aprobación mientras Codex trabaja.">
  </picture>
</p>

## Qué hace Kodosi

### Mira qué agente te necesita

Claude Code, Codex y otros agentes de programación se detienen y te esperan. Kodosi reúne los
terminales de todos tus ordenadores en una ventana, y cada uno muestra si su agente trabaja,
espera tu respuesta, ha terminado o ha fallado. Una notificación te avisa cuando no estás
delante, también en tu teléfono. Tus agentes se ejecutan tal cual, con sus propios ajustes e
inicio de sesión.

### Pregunta qué te necesita

Dosi, un agente para tus agentes, lee tus terminales, te dice qué te está esperando y, cuando
tú lo decides, responde por ti en el terminal.

<p align="center">
  <picture>
    <source media="(prefers-reduced-motion: reduce)" srcset="../media/dosi-still.webp">
    <img src="../media/dosi.webp" width="900" alt="Cuando le preguntas qué te necesita, Dosi lee los terminales, dice que checkout espera una aprobación y pregunta si debe permitirla.">
  </picture>
</p>

### Mira lo que crea tu agente

Sus planes, páginas y documentos se abren en un lienzo al lado del terminal, y tú editas los
documentos junto con tus amigos y el agente.

<p align="center">
  <picture>
    <source media="(prefers-reduced-motion: reduce)" srcset="../media/canvas-still.webp">
    <img src="../media/canvas.webp" width="900" alt="El terminal de Claude Code con su lienzo: un plan de lanzamiento que Leo edita mientras Claude sugiere un cambio, luego una hoja de presupuesto, diapositivas, un PDF y un navegador compartido donde Claude añade una taza al carrito.">
  </picture>
</p>

### Trabaja en equipo

Dale a un amigo un terminal en marcha, o reúne terminales, una conversación y tareas en un
proyecto para personas y sus agentes.

<p align="center"><img src="../media/project.webp" width="900" alt="Un proyecto con un terminal compartido y una conversación de personas y agentes."></p>

### Privado y abierto

Los terminales y los proyectos están cifrados de extremo a extremo con criptografía
poscuántica, así que el servicio no puede leerlos. Kodosi es de código abierto y puedes
ejecutar el servicio por tu cuenta.
[Seguridad](SECURITY.md) · [Alojamiento propio](../SELF-HOSTING.md) (en inglés)

## Descargar Kodosi

| | Requisitos | Instalación |
| --- | --- | --- |
| **Mac** | Apple silicon, macOS 26 o posterior | Abre la imagen de disco y mueve Kodosi a Aplicaciones, o ejecuta `brew install --cask johnkozaris/tap/kodosi`. La app se actualiza sola. |
| **Linux** | x86-64, Ubuntu 24.04 o Debian 13 o posterior | Instala el paquete Debian. Añade el repositorio apt de Kodosi, así que las actualizaciones del sistema también actualizan Kodosi. La [versión](https://github.com/johnkozaris/Kodosi/releases/latest) también tiene un paquete para Arch y un archivo portable. |
| **Windows** | x64, Windows 11 24H2 o posterior | Antes de abrir la descarga por primera vez, añade como administrador el [certificado de Kodosi](https://updates.kodosi.com/windows/Kodosi.cer) a Equipo local › Personas de confianza. La app se actualiza sola. |
| **Android** | ARM64, Android 15 o posterior | Descarga el APK en el dispositivo, ábrelo y permite la instalación cuando Android lo pida. Una versión nueva se instala igual. |
| **iPhone y iPad** | iOS 26 o posterior | Compila Kodosi para tu [iPhone o iPad](../../clients/ios/README.md). |

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
