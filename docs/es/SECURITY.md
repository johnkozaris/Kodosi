# Seguridad

[English](../SECURITY.md) · [Deutsch](../de/SECURITY.md) · [Ελληνικά](../el/SECURITY.md) · **Español** · [Français](../fr/SECURITY.md) · [Italiano](../it/SECURITY.md) · [日本語](../ja/SECURITY.md) · [한국어](../ko/SECURITY.md) · [简体中文](../zh-Hans/SECURITY.md)

El tráfico de los terminales y el contenido de los proyectos se cifran de extremo a extremo en tus
dispositivos, y las claves privadas nunca salen de ellos sin cifrar. El servidor enruta las
conexiones y guarda los registros cifrados de los proyectos; nunca ejecuta comandos ni ve su texto
en claro. Para ver el modelo de amenazas completo, los mecanismos criptográficos y las limitaciones
conocidas, consulta el [diseño del cifrado](../CRYPTOGRAPHY.md) (en inglés). Ningún tercero
independiente ha auditado todavía este diseño ni su implementación.

## Qué permite compartir

Compartir un terminal concede a los destinatarios control total con tus privilegios del sistema
operativo en ese ordenador. Compartir un terminal en un proyecto lo pone a disposición de todos tus
amigos en ese proyecto, incluidos los que se unan más adelante. Como un terminal compartido se
ejecuta como tú, cualquiera que entre en él también puede ejecutar `kodosi` como tú y leer o
modificar archivos en esa máquina. Comparte terminales solo con personas a quienes confíes ese
ordenador.

Kodosi no restringe lo que los participantes o sus agentes pueden hacer dentro de un terminal o
proyecto compartido. Eliminar a un amigo revoca de inmediato su acceso a tus terminales y saca a
ambos de los proyectos que pertenezcan al otro. Bloquear a un usuario hace lo mismo e impide
futuras solicitudes de amistad.

Antes de que alguien nuevo entre en tus terminales o proyectos, Kodosi te pregunta en la app del
dispositivo donde empieza el cambio, también cuando lo pide el comando `kodosi`, por ejemplo para un
agente. Esto protege frente a errores y agentes; quien escribe en un terminal que compartiste actúa
como tú y puede saltarse esta pregunta.

Cada uno de tus dispositivos aprobados puede controlar todos tus terminales, elegir con quién los
compartes, iniciar otros nuevos en tus demás ordenadores que ejecuten Kodosi y escribir a Dosi en
esos ordenadores (donde Dosi actúa con el mismo acceso local que cuando lo usas directamente).
Quitar un dispositivo revoca su acceso a los terminales y a Dosi.

### Lienzo y navegador compartido

Todas las personas con acceso a un terminal pueden ver y editar los archivos de su lienzo y usar su
navegador compartido, así que inicia sesión en un navegador compartido solo en cuentas que todos
los participantes deban poder usar. Los terminales compartidos nunca reutilizan las sesiones de
navegador que tengas guardadas en terminales privados. Los archivos del lienzo son archivos
normales en el ordenador anfitrión.

Las páginas HTML y las vistas de documentos en directo se ejecutan dentro de un marco aislado sin
acceso a la red, sin acceso a archivos y sin almacenamiento persistente. Mientras un navegador
compartido está activo, su puerto de depuración y el puerto del controlador de cada ventana
escuchan en `127.0.0.1` o `::1`, lo que significa que otras cuentas locales del sistema operativo
en esa misma máquina pueden acceder a ellos. Los sitios en el navegador compartido pueden guardar
como máximo una descarga en la carpeta del terminal por cada acción de un usuario o agente, y solo
acceden a tu portapapeles cuando copias de forma explícita.

En Linux, Kodosi solo renderiza páginas donde puede ejecutarse el sandbox de Chromium. En Ubuntu,
Kodosi incluye un perfil de AppArmor que permite a la app, a su proceso de renderizado y a Kodosi
Browser crear los user namespaces que necesita el sandbox de Chromium. Por el mismo motivo, el
navegador compartido no se ejecuta como root en Linux.

Las apps de Kodosi no incluyen Kodosi Browser de fábrica. La primera vez que abres un navegador
compartido en un ordenador, Kodosi lo descarga desde las versiones de Kodosi publicadas en GitHub, y
busca actualizaciones a diario en el feed de actualizaciones de Kodosi, en `updates.kodosi.com`. Las
descargas solo se aceptan si el feed de actualizaciones lleva una firma válida de la clave de
actualizaciones de Kodosi y el SHA-256 del archivo coincide con el del feed.

### Qué envía Kodosi Browser a Google

Kodosi Browser es Chromium, que se conecta por sí solo a estos servicios de Google, sin enviar tus
cookies a ninguno de ellos:

- Actualizaciones de sus componentes, como la lista de certificados revocados: aproximadamente cada
  cinco horas pregunta a `update.googleapis.com` si hay versiones nuevas, y las descarga de los
  servidores de descarga de Google (como `edgedl.me.gvt1.com`). Envía el ID y la versión de cada
  componente, su propia versión, y el sistema operativo y el procesador, pero ningún dato de
  navegación.
- Navegación segura: mantiene listas de sitios peligrosos obtenidas de
  `safebrowsing.googleapis.com`, y solo envía el estado de sus listas. Cuando la dirección de una
  página coincide con una lista, envía los primeros 4 bytes de un hash de la dirección, nunca la
  dirección. Si la comprobación de phishing que hace en tu ordenador considera que una página es de
  phishing, envía la dirección de esa página a `sb-ssl.google.com` para confirmarlo.
- Hora de la red: cuando no está seguro de la hora, pregunta a `clients2.google.com`, que le indica
  si el certificado de un sitio ha caducado aunque tu reloj esté mal. La solicitud no contiene nada
  sobre ti.

La opción **Predicciones de formularios de Google** permanece desactivada hasta que la actives en
**Ajustes › Navegador**. Una vez activada, Kodosi Browser envía a `content-autofill.googleapis.com`
un hash de cada formulario que muestra, calculado a partir del nombre del sitio y de los nombres de
los campos del formulario, y Google responde para qué sirve cada campo. Cuando envías un formulario,
también indica qué tipo de datos contenía cada campo, nunca lo que escribiste. Kodosi Browser nunca
se conecta a `www.google.com` antes de una búsqueda, y nunca pregunta a Google, con tus cookies de
Google, si la propia barra de direcciones de Chrome puede ofrecer el Modo IA.

La opción **Cookies entre sitios** también permanece desactivada hasta que la actives en
**Ajustes › Navegador**. Mientras está desactivada, un sitio que aparece dentro de la página de otro
sitio, como un vídeo de YouTube o un botón de inicio de sesión, no recibe allí sus cookies, así que
no puede reconocerte en los distintos sitios que visitas.

## Conexiones directas

Cuando dos de tus propios dispositivos pueden comunicarse directamente entre sí, en la misma red
local o mediante NAT traversal, el tráfico del terminal viaja directamente entre ambos con el mismo
cifrado de extremo a extremo en lugar de pasar por el servidor de retransmisión. Ambos dispositivos
ven las direcciones IP del otro.

Para descubrir su dirección pública, Kodosi consulta un servidor STUN de Cloudflare, que observa
esa IP. Mientras busca o mantiene una conexión directa (y durante un minuto después), el
dispositivo mantiene abierto un único puerto UDP y solo acepta conexiones procedentes de tus
propios dispositivos aprobados. Los terminales compartidos con otras personas pasan siempre por el
servidor de retransmisión, de modo que otros usuarios nunca ven tus direcciones IP.

## Qué ve el servicio

El servidor ve los registros de cuentas y dispositivos, la pertenencia a proyectos, los metadatos
de proyectos y terminales, el tipo de agente y el estado comunicado de cada terminal (como
trabajando o esperando aprobación, nunca el texto del terminal), si ya has visto ese estado, las
cabeceras de enrutamiento, los tiempos y los tamaños del texto cifrado (salvo en el tráfico que
sigue una ruta directa punto a punto). También ve el estado de actividad de cada dispositivo (en
Kodosi, en otra app de escritorio o inactivo) para que las notificaciones lleguen únicamente a un
dispositivo activo. No ve cómo se reactivan tus ordenadores: sus direcciones de red y sus ajustes de
reactivación solo viajan entre tus propios dispositivos, cifrados de extremo a extremo.

Las notificaciones push móviles viajan a través de APNs de Apple o FCM de Google. Esos proveedores
ven los ID de los terminales que están esperando, han terminado o han fallado junto con su tipo de
agente y su estado, nunca los nombres ni el contenido de los terminales.

El cifrado no oculta estos metadatos ni protege frente a caídas del servidor. Quitar un dispositivo
corta su acceso a partir de ese momento; si pierdes o te roban un dispositivo, genera también una
clave de recuperación nueva para dejar fuera ese dispositivo. Si alguien inicia una identidad nueva
en tu cuenta mientras sigues teniendo dispositivos activos, el servidor la retrasa 24 horas y avisa
a tus dispositivos actuales para que puedas cancelarla. Las credenciales de GitHub y Gitea
permanecen en tu ordenador.

Eliminar tu cuenta borra sus dispositivos, datos de recuperación, lista de amigos, elementos
compartidos activos y proyectos propios. Los mensajes que publicaste en proyectos de otras personas
permanecen sin una cuenta asociada. El servidor conserva durante 30 días el emisor y el sujeto del
inicio de sesión eliminado para rechazar tokens emitidos anteriormente.

## Comprobar una descarga

Cada versión incluye `SHA256SUMS` (el resumen SHA-256 de cada artefacto) y `SHA256SUMS.asc`,
firmado con la misma clave de Kodosi que firma el repositorio apt. Coloca ambos archivos junto a tu
descarga y ejecuta:

```sh
curl -fsSLO https://updates.kodosi.com/apt/kodosi-archive-keyring.pgp
gpgv --keyring ./kodosi-archive-keyring.pgp SHA256SUMS.asc SHA256SUMS
shasum -a 256 --ignore-missing -c SHA256SUMS
```

`gpgv` (de GnuPG) debe indicar una firma válida de la clave
`E8007A36D199B6EE755B1EE9C612C00442E52299`, y `shasum` debe mostrar `OK` para tu archivo.

## Informar de una vulnerabilidad

Usa el [informe privado de vulnerabilidades de GitHub](https://github.com/johnkozaris/Kodosi/security/advisories/new)
o escribe a [security@kodosi.com](mailto:security@kodosi.com) indicando el commit o la versión, los
pasos para reproducirla, el impacto y los logs sin datos sensibles. Por favor, mantén los detalles
de vulnerabilidades fuera de las issues públicas.
