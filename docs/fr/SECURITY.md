# Sécurité

[English](../SECURITY.md) · [Deutsch](../de/SECURITY.md) · [Ελληνικά](../el/SECURITY.md) · [Español](../es/SECURITY.md) · **Français** · [Italiano](../it/SECURITY.md) · [日本語](../ja/SECURITY.md) · [한국어](../ko/SECURITY.md) · [简体中文](../zh-Hans/SECURITY.md)

Le trafic des terminaux et le contenu des projets sont chiffrés de bout en bout par vos appareils,
et les clés privées ne quittent jamais vos appareils sans être chiffrées. Le backend achemine les
connexions et stocke les données chiffrées des projets ; il n'exécute jamais de commandes et n'en
voit jamais le texte en clair. Pour le modèle de menaces complet, les mécanismes cryptographiques et
les limites connues, consultez la [conception du chiffrement](../CRYPTOGRAPHY.md) (en anglais).
Aucun tiers indépendant n'a encore audité cette conception ni son implémentation.

## Ce que donne le partage

Partager un terminal donne aux destinataires le contrôle total, avec vos privilèges système sur cet
ordinateur. Partager un terminal dans un projet le rend accessible à tous vos amis présents dans ce
projet, y compris ceux qui le rejoignent plus tard. Comme un terminal partagé s'exécute sous votre
identité, toute personne qui y a accès peut aussi exécuter `kodosi` en votre nom, ainsi que lire ou
modifier des fichiers sur cette machine. Ne partagez des terminaux qu'avec des personnes à qui vous
faites confiance pour cet ordinateur.

Kodosi ne restreint pas ce que les participants ou leurs agents peuvent faire dans un terminal ou
un projet partagé. Retirer un ami révoque immédiatement l'accès à vos terminaux et retire chacun de
vous des projets appartenant à l'autre. Bloquer un utilisateur a le même effet et empêche toute
future demande d'ami.

Avant que quelqu'un de nouveau n'accède à vos terminaux ou à vos projets, Kodosi vous le demande
dans l'application de l'appareil où le changement commence, y compris quand la commande `kodosi` le
demande, par exemple pour un agent. Cela protège contre les erreurs et les agents ; une personne qui
tape dans un terminal que vous avez partagé agit en votre nom et peut contourner cette question.

Chacun de vos appareils approuvés peut contrôler tous vos terminaux, choisir avec qui ils sont
partagés, en lancer de nouveaux sur vos autres ordinateurs exécutant Kodosi et envoyer des messages
à Dosi sur ces ordinateurs (où Dosi agit avec le même accès local que lorsque vous l'utilisez
directement). Retirer un appareil révoque son accès aux terminaux et à Dosi.

### Canvas et navigateur partagé

Toute personne ayant accès à un terminal peut consulter et modifier les fichiers de son canvas et
utiliser son navigateur partagé : ne vous connectez donc dans un navigateur partagé qu'aux comptes
que chaque participant est autorisé à utiliser. Les terminaux partagés ne réutilisent jamais les
connexions de navigateur enregistrées dans vos terminaux privés. Les fichiers du canvas sont des
fichiers ordinaires sur l'ordinateur hôte.

Les pages HTML et les vues de documents en direct s'exécutent dans un cadre isolé sans accès réseau,
sans accès aux fichiers et sans stockage persistant. Tant qu'un navigateur partagé est éveillé, son
port de débogage et le port de pilotage de chaque fenêtre écoutent sur `127.0.0.1` ou `::1`, ce qui
signifie que d'autres comptes utilisateurs locaux sur cette même machine peuvent y accéder. Les
sites ouverts dans le navigateur partagé peuvent enregistrer au maximum un téléchargement dans le
dossier du terminal par action d'un utilisateur ou d'un agent, et ne peuvent accéder à votre
presse-papiers que lorsque vous copiez explicitement.

Sous Linux, Kodosi n'affiche des pages que là où le bac à sable de Chromium peut fonctionner. Sous
Ubuntu, Kodosi fournit un profil AppArmor autorisant l'app, son processus de rendu et Kodosi Browser
à créer les user namespaces requis par le bac à sable de Chromium. Pour la même raison, le
navigateur partagé ne s'exécute pas en root sous Linux.

Les apps Kodosi n'embarquent pas Kodosi Browser. La première fois que vous ouvrez un navigateur
partagé sur un ordinateur, Kodosi le télécharge depuis les versions de Kodosi publiées sur GitHub,
et vérifie chaque jour dans le flux de mise à jour de Kodosi, sur `updates.kodosi.com`, si des mises
à jour sont disponibles. Un téléchargement n'est accepté que si le flux de mise à jour porte une
signature valide de la clé de mise à jour de Kodosi et si le condensat SHA-256 du fichier correspond
à celui indiqué par le flux.

### Ce que Kodosi Browser envoie à Google

Kodosi Browser est Chromium, qui contacte de lui-même les services de Google suivants, sans jamais
leur transmettre vos cookies :

- Mises à jour de ses composants, comme la liste des certificats révoqués : environ toutes les cinq
  heures, il demande à `update.googleapis.com` s'il existe de nouvelles versions, et les télécharge
  depuis les serveurs de téléchargement de Google (comme `edgedl.me.gvt1.com`). Il envoie
  l'identifiant et la version de chaque composant, sa propre version, ainsi que le système
  d'exploitation et le processeur, mais aucune donnée de navigation.
- Navigation sécurisée : il conserve des listes de sites dangereux provenant de
  `safebrowsing.googleapis.com`, en n'envoyant que l'état de ses listes. Quand l'adresse d'une page
  correspond à une liste, il envoie les 4 premiers octets d'un hachage de l'adresse, jamais
  l'adresse elle-même. Si la détection d'hameçonnage qu'il effectue sur votre ordinateur considère
  qu'une page relève de l'hameçonnage, il envoie l'adresse de cette page à `sb-ssl.google.com` pour
  confirmation.
- Heure réseau : quand il n'est pas sûr de l'heure, il interroge `clients2.google.com`, qui lui
  indique si le certificat d'un site a expiré, même si votre horloge est déréglée. La requête ne
  contient rien sur vous.

L'option **Prédictions de formulaires par Google** reste désactivée tant que vous ne l'activez pas
dans **Réglages › Navigateur**. Une fois l'option activée, Kodosi Browser envoie à
`content-autofill.googleapis.com` un hachage de chaque formulaire qu'il affiche, calculé à partir du
nom du site et des noms des champs du formulaire, et Google indique en retour à quoi sert chaque
champ. Quand vous envoyez un formulaire, il indique aussi quel type de données contenait chaque
champ, jamais ce que vous avez saisi. Kodosi Browser ne se connecte jamais à `www.google.com` en
prévision d'une recherche, et ne demande jamais à Google, avec vos cookies Google, si la barre
d'adresse de Chrome elle-même peut proposer le Mode IA.

L'option **Cookies intersites** reste elle aussi désactivée tant que vous ne l'activez pas dans
**Réglages › Navigateur**. Tant qu'elle est désactivée, un site affiché dans la page d'un autre
site, comme une vidéo YouTube ou un bouton de connexion, n'y reçoit pas ses cookies, et ne peut donc
pas vous reconnaître sur les différents sites que vous visitez.

### L'écran d'un ordinateur

Activer le partage d'écran dans les réglages Kodosi d'un ordinateur permet à chacun de vos appareils
approuvés de voir et de contrôler tout l'écran de cet ordinateur, pas seulement ses terminaux,
depuis le canvas de ses terminaux. Vos amis ne le voient jamais, même dans un terminal que vous
partagez avec eux, et la vidéo, les saisies et le texte du presse-papiers circulent chiffrés de bout
en bout comme les terminaux eux-mêmes. Une partie distincte de Kodosi se charge de la capture et des
saisies. Sur un Mac, elle détient les autorisations d'enregistrement de l'écran et d'accessibilité
nécessaires, afin que les programmes de vos terminaux ne les obtiennent pas. Sous Linux, le bureau
accorde le partage à Kodosi via son portail de partage d'écran et affiche sa propre notification
pendant que l'écran est partagé. Un bureau sous X11 n'a pas ce portail et laisse chaque programme de
votre session voir et contrôler l'écran ; il suffit alors d'activer le partage dans Kodosi. Tant que
quelqu'un regarde, la barre des menus ou la zone de notification de l'ordinateur indique quels
appareils regardent ; désactiver le partage, ou cliquer sur **Arrêter le partage** à cet endroit,
met fin à chaque affichage.

Sur un Mac, cette partie de Kodosi demande en outre à la personne devant le Mac, dans sa propre
alerte, avant de servir un compte : une fois pour chaque compte, quand le partage est activé. Aucun
programme ne peut faire apparaître cette alerte : seul votre propre clic sur l'interrupteur de
Kodosi le peut. L'alerte nomme le compte, et son serveur quand ce n'est pas celui de Kodosi : un
programme du Mac qui lance son propre Kodosi avec un autre compte n'obtient donc rien tant que cette
personne n'autorise pas ce compte. Seule cette partie de Kodosi peut lire ou modifier les réponses,
qui restent sur ce Mac, et désactiver le partage, ou **Arrêter le partage**, les retire toutes.

Pendant qu'un appareil contrôle l'écran, le texte copié sur l'ordinateur va vers cet appareil, et un
collage sur celui-ci donne à l'ordinateur le texte de cet appareil. Le texte qu'un gestionnaire de
mots de passe marque comme privé reste sur l'ordinateur. Lorsque la personne devant l'ordinateur
utilise sa souris ou son clavier, les saisies des personnes qui regardent patientent un instant, et
un écran verrouillé reste verrouillé : ces personnes voient qu'il est verrouillé, pas ce qu'il
affiche.

## Connexions directes

Lorsque deux de vos propres appareils peuvent se joindre directement, sur le même réseau local ou
par traversée de NAT, le trafic des terminaux circule directement entre eux avec le même
chiffrement de bout en bout au lieu de passer par le serveur relais. Les deux appareils voient
alors leurs adresses IP respectives. Lorsqu'ils ne le peuvent pas, le trafic passe en UDP par le
serveur relais avec le même chiffrement, et aucun des deux appareils ne voit l'adresse IP de l'autre.

Pour déterminer son adresse publique, Kodosi interroge le serveur Kodosi, qui voit de toute façon cette
adresse IP. Lorsqu'un appareil recherche ou maintient l'une de ces connexions (et pendant une minute
ensuite), il garde un unique port UDP ouvert et n'accepte de connexions que
d'appareils qui affichent l'un de vos terminaux. Les terminaux partagés avec d'autres personnes passent toujours par le serveur
relais, de sorte que les autres utilisateurs ne voient jamais vos adresses IP.

## Ce que voit le service

Le backend voit les enregistrements de comptes et d'appareils, l'appartenance aux projets, les
métadonnées des projets et des terminaux, le type d'agent et l'état signalé de chaque terminal
(par exemple en cours ou en attente d'approbation, jamais le texte du terminal), si vous avez déjà
consulté cet état, les en-têtes de routage, l'horodatage et la taille des textes chiffrés (sauf
pour le trafic qui emprunte un chemin direct en pair à pair). Il voit également l'état d'activité
de chaque appareil (dans Kodosi, dans une autre app de bureau ou inactif) afin d'acheminer les
notifications uniquement vers un appareil actif. Il ne voit pas comment vos ordinateurs se
réveillent : leurs adresses réseau et leurs réglages de réveil ne circulent qu'entre vos propres
appareils, chiffrés de bout en bout.

Les notifications push mobiles transitent par APNs d'Apple ou FCM de Google. Ces fournisseurs
voient les identifiants des terminaux en attente, terminés ou en échec, ainsi que leur type d'agent
et leur état, jamais le nom ni le contenu des terminaux.

Le chiffrement ne masque pas ces métadonnées et ne protège pas contre les pannes serveur. Retirer un
appareil coupe son accès à partir de ce moment ; si un appareil est perdu ou volé, générez également
une nouvelle clé de récupération pour tenir cet appareil à l'écart. Si quelqu'un crée une nouvelle
identité sur votre compte alors que vous disposez encore d'appareils actifs, le backend retarde
l'opération de 24 heures et avertit vos appareils existants afin que vous puissiez l'annuler. Les
identifiants GitHub et Gitea restent sur votre machine.

Supprimer votre compte efface ses appareils, ses données de récupération, sa liste d'amis, ses
partages actifs et les projets dont vous êtes propriétaire. Les messages que vous avez publiés dans
les projets d'autres personnes subsistent sans compte associé. Le backend conserve l'émetteur et le
sujet de la connexion supprimée pendant 30 jours afin de rejeter les jetons émis précédemment.

## Vérifier un téléchargement

Chaque version inclut `SHA256SUMS` (le condensat SHA-256 de chaque fichier distribué) et
`SHA256SUMS.asc`, signé avec la même clé Kodosi que celle qui signe le dépôt apt. Placez les deux
fichiers à côté de votre téléchargement et exécutez :

```sh
curl -fsSLO https://updates.kodosi.com/apt/kodosi-archive-keyring.pgp
gpgv --keyring ./kodosi-archive-keyring.pgp SHA256SUMS.asc SHA256SUMS
shasum -a 256 --ignore-missing -c SHA256SUMS
```

`gpgv` (fourni par GnuPG) doit indiquer une signature valide de la clé
`E8007A36D199B6EE755B1EE9C612C00442E52299`, et `shasum` doit afficher `OK` pour votre fichier.

## Signaler une vulnérabilité

Utilisez le [signalement privé de vulnérabilités sur GitHub](https://github.com/johnkozaris/Kodosi/security/advisories/new)
ou écrivez à [security@kodosi.com](mailto:security@kodosi.com) en indiquant le commit ou la version,
les étapes de reproduction, l'impact et des journaux expurgés. Merci de ne pas publier les détails
d'une vulnérabilité dans les issues publiques.
