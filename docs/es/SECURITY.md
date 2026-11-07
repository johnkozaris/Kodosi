# Seguridad

[English](../SECURITY.md) · [Deutsch](../de/SECURITY.md) · [Ελληνικά](../el/SECURITY.md) · **Español** · [Français](../fr/SECURITY.md) · [Italiano](../it/SECURITY.md) · [日本語](../ja/SECURITY.md) · [한국어](../ko/SECURITY.md) · [简体中文](../zh-Hans/SECURITY.md)

Tus dispositivos cifran el tráfico de los terminales y el contenido de las salas de extremo a
extremo. Las claves privadas se quedan en los dispositivos. El backend enruta las conexiones y
guarda los datos cifrados de las salas; nunca ejecuta comandos ni ve su texto en claro. El
[diseño del cifrado](../CRYPTOGRAPHY.md) (en inglés) describe el modelo de amenazas, los
mecanismos y los límites conocidos; todavía no lo ha auditado ninguna parte independiente.

## Qué permite compartir

Compartir un terminal da el control total, con los privilegios del sistema operativo del
usuario del host. Todas las personas de una sala pueden usar los terminales compartidos de la
sala, también los miembros invitados más tarde. Compartir un terminal no comparte nada más.
Kodosi confía en todas las personas que comparten un terminal o una sala, y en los programas y
agentes de sus terminales, y no limita lo que hacen. Cada uno de tus dispositivos aprobados
controla todos tus terminales y puede iniciar otros nuevos en tus demás ordenadores que
ejecutan Kodosi.

Todas las personas que pueden abrir un terminal también ven los archivos de su lienzo y usan
su navegador compartido, así que inicia sesión allí solo donde todas puedan actuar. Un
terminal compartido nunca tiene tus sesiones guardadas. Los archivos de un lienzo son archivos
normales, y todas las personas que pueden abrir el terminal pueden editarlos. Una página en el
lienzo y un documento en directo se ejecutan en un marco cerrado, sin red, sin archivos y sin
almacenamiento que perdure. Mientras un navegador compartido está despierto, su puerto de
depuración escucha en `127.0.0.1`, donde pueden alcanzarlo otras cuentas de ese ordenador.
Cuando un dispositivo abre el navegador de un terminal de otro ordenador, ese ordenador escucha
en un puerto TCP durante unos segundos para que los dos se conecten directamente. Solo pasa
ese dispositivo, con el cifrado del terminal. La app de Linux muestra páginas solo donde
Chromium puede aislarlas en un sandbox; en Ubuntu, su perfil de AppArmor permite a la app y a
su proceso de páginas crear los user namespaces que necesita el sandbox.

## Qué ve el servicio

El servicio ve los registros de cuentas y dispositivos, los miembros de las salas, los
metadatos de salas y terminales, el agente y el estado de cada terminal (como trabajando o
esperando una aprobación, nunca su texto), los campos de enrutamiento, los tiempos y los
tamaños del texto cifrado. El cifrado no oculta estos metadatos ni evita una caída del
servicio. Un dispositivo que quitas pierde el acceso desde ese momento. Después de quitar un
dispositivo perdido o robado, crea una clave de recuperación nueva para mantenerlo fuera. Las
credenciales de GitHub y Gitea se quedan en tu ordenador.

Eliminar una cuenta quita sus dispositivos, sus datos de recuperación, sus amigos, lo que
comparte y las salas que son suyas; los mensajes en salas de otras personas se quedan sin
cuenta. Durante 30 días, el servicio guarda el emisor y el sujeto del inicio de sesión
eliminado para rechazar tokens anteriores.

## Informar de una vulnerabilidad

Usa el [aviso privado de vulnerabilidades](https://github.com/johnkozaris/Kodosi/security/advisories/new)
o escribe a [security@kodosi.com](mailto:security@kodosi.com) con la revisión, los pasos, el
impacto y los logs sin datos sensibles. No pongas detalles en issues públicas.
