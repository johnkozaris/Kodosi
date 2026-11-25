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
partagé sur un ordinateur, Kodosi le télécharge depuis `updates.kodosi.com` et vérifie
quotidiennement la disponibilité de mises à jour. Un téléchargement n'est accepté que si le flux de
mise à jour porte une signature valide de la clé de mise à jour de Kodosi et si le condensat
SHA-256 du fichier correspond à celui indiqué par le flux.

## Connexions directes

Lorsque deux de vos propres appareils peuvent se joindre directement, sur le même réseau local ou
par traversée de NAT, le trafic des terminaux circule directement entre eux avec le même
chiffrement de bout en bout au lieu de passer par le serveur relais. Les deux appareils voient
alors leurs adresses IP respectives.

Pour déterminer son adresse publique, Kodosi interroge un serveur STUN de Cloudflare, qui voit cette
adresse IP. Lorsqu'un appareil recherche ou maintient une connexion directe (et pendant une minute
ensuite), il garde un unique port UDP ouvert et n'accepte de connexions que de vos propres
appareils approuvés. Les terminaux partagés avec d'autres personnes passent toujours par le serveur
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
