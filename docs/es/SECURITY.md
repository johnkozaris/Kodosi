# Seguridad

[English](../SECURITY.md) · [Deutsch](../de/SECURITY.md) · [Ελληνικά](../el/SECURITY.md) · **Español** · [Français](../fr/SECURITY.md) · [Italiano](../it/SECURITY.md) · [日本語](../ja/SECURITY.md) · [한국어](../ko/SECURITY.md) · [简体中文](../zh-Hans/SECURITY.md)

Tus dispositivos cifran el tráfico de los terminales y el contenido de los proyectos de extremo a
extremo. Las claves privadas se quedan en los dispositivos. El backend enruta las conexiones y
guarda los datos cifrados de los proyectos; nunca ejecuta comandos ni ve su texto en claro. El
[diseño del cifrado](../CRYPTOGRAPHY.md) (en inglés) describe el modelo de amenazas, los
mecanismos y los límites conocidos; todavía no lo ha auditado ninguna parte independiente.

## Qué permite compartir

Compartir un terminal da el control total, con los privilegios del sistema operativo de su dueño en ese ordenador. Un terminal en un proyecto es para los amigos de su dueño en ese proyecto, también los amigos que se unan más tarde. Cuando eliminas a un amigo, pierde tus
terminales, y cada uno sale de los proyectos del otro. Bloquear a una persona hace lo mismo, y no puede enviarte solicitudes de amistad. Un terminal compartido se ejecuta como tú: las personas en él también pueden ejecutar `kodosi` como tú y leer y cambiar tus archivos en ese
ordenador, así que compártelo solo con personas a quienes confíes ese ordenador. Kodosi confía en todas las personas que comparten un terminal o un proyecto, y en los programas y agentes de sus terminales, y no limita lo que hacen. Cada uno de tus dispositivos aprobados controla
todos tus terminales y puede iniciar otros nuevos en tus demás ordenadores que ejecutan Kodosi. Desde cualquiera de tus dispositivos puedes escribir a Dosi en tus ordenadores, y Dosi actúa allí igual que cuando le escribes en el propio ordenador. Un dispositivo que quitas ya no
puede escribir a Dosi.

Todas las personas que pueden abrir un terminal también ven los archivos de su lienzo y usan su
navegador compartido, así que inicia sesión allí solo donde todas puedan actuar. El navegador
compartido de un terminal compartido nunca usa tus sesiones guardadas. Los archivos de un lienzo son
archivos normales, y todas las personas que pueden abrir el terminal pueden editarlos. Una página en
el lienzo y un documento en directo se ejecutan en un marco cerrado, sin red, sin archivos y sin
almacenamiento que perdure. Mientras un navegador compartido está despierto, su puerto de depuración y
el puerto de driver de cada ventana escuchan en `127.0.0.1` o `::1`, donde pueden alcanzarlos otras
cuentas de ese ordenador. Un sitio en el navegador compartido guarda una descarga en la carpeta del
terminal por cada acción de una persona o de un agente, y solo llega a tu portapapeles cuando copias.
Cuando un dispositivo abre el navegador de un terminal de otro ordenador, ese ordenador escucha en un
puerto TCP durante unos segundos para que los dos se conecten directamente. Solo pasa ese dispositivo,
con el cifrado del terminal. La app de Linux muestra páginas solo donde Chromium puede aislarlas en un
sandbox; en Ubuntu, su perfil de AppArmor permite a la app, a su proceso de páginas y a Kodosi Browser
crear los user namespaces que necesita el sandbox. Por la misma razón, el navegador compartido no se
ejecuta como root en Linux. Las apps no incluyen Kodosi Browser. Cuando un ordenador abre su primer
navegador compartido, Kodosi descarga el navegador del feed de `updates.kodosi.com` y busca allí una
versión nueva cada día. Kodosi solo usa una descarga cuando el feed lleva la firma de la clave de las
actualizaciones de Kodosi y la descarga tiene el SHA-256 que indica el feed.

## Qué ve el servicio

El servicio ve los registros de cuentas y dispositivos, los miembros de los proyectos, los
metadatos de proyectos y terminales, el agente y el estado de cada terminal (como trabajando o
esperando una aprobación, nunca su texto), los campos de enrutamiento, los tiempos y los
tamaños del texto cifrado. Las notificaciones push a tu iPhone, iPad o dispositivo Android
pasan por el servicio push de Apple o de Google, que ve el ID del terminal, su agente y su
estado, nunca nombres ni texto. El cifrado no oculta estos metadatos ni evita una caída del
servicio. Un dispositivo que quitas pierde el acceso desde ese momento. Después de quitar un
dispositivo perdido o robado, crea una clave de recuperación nueva para mantenerlo fuera.
Cuando alguien empieza de cero en tu cuenta mientras tienes dispositivos, eso espera un día y
tus dispositivos lo muestran, para que puedas detenerlo. Las credenciales de GitHub y Gitea se
quedan en tu ordenador.

Eliminar una cuenta quita sus dispositivos, sus datos de recuperación, sus amigos, lo que
comparte y los proyectos que son suyos; los mensajes en proyectos de otras personas se quedan sin
cuenta. Durante 30 días, el servicio guarda el emisor y el sujeto del inicio de sesión
eliminado para rechazar tokens anteriores.

## Comprobar una descarga

Cada versión tiene `SHA256SUMS`, el SHA-256 de cada uno de sus archivos, y `SHA256SUMS.asc`,
la firma de esa lista con la clave de Kodosi que también firma el repositorio apt. Pon los dos
archivos junto a tu descarga y ejecuta:

```sh
curl -fsSLO https://updates.kodosi.com/apt/kodosi-archive-keyring.pgp
gpgv --keyring ./kodosi-archive-keyring.pgp SHA256SUMS.asc SHA256SUMS
shasum -a 256 --ignore-missing -c SHA256SUMS
```

`gpgv`, de GnuPG, tiene que indicar una firma válida de la clave
`E8007A36D199B6EE755B1EE9C612C00442E52299`, y `shasum` tiene que mostrar OK para tu descarga.

## Informar de una vulnerabilidad

Usa el [aviso privado de vulnerabilidades](https://github.com/johnkozaris/Kodosi/security/advisories/new)
o escribe a [security@kodosi.com](mailto:security@kodosi.com) con la revisión, los pasos, el
impacto y los logs sin datos sensibles. No pongas detalles en issues públicas.
