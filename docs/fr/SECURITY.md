# Sécurité

[English](../SECURITY.md) · [Deutsch](../de/SECURITY.md) · [Ελληνικά](../el/SECURITY.md) · [Español](../es/SECURITY.md) · **Français** · [Italiano](../it/SECURITY.md) · [日本語](../ja/SECURITY.md) · [한국어](../ko/SECURITY.md) · [简体中文](../zh-Hans/SECURITY.md)

Vos appareils chiffrent le trafic des terminaux et le contenu des projets de bout en bout. Les
clés privées restent sur les appareils. Le backend achemine les connexions et stocke les
données chiffrées des projets ; il n'exécute jamais de commandes et n'en voit jamais le texte
en clair. La [conception du chiffrement](../CRYPTOGRAPHY.md) (en anglais) décrit le modèle de
menaces, les mécanismes et les limites connues ; aucun organisme indépendant ne l'a encore
auditée.

## Ce que donne le partage

Partager un terminal donne le contrôle total, avec les privilèges système de l'utilisateur de
l'hôte. Un terminal dans un projet va aux amis de son propriétaire dans ce projet, y compris
aux amis qui le rejoignent plus tard. Quand vous retirez un ami, il perd vos terminaux, et
chacun de vous quitte les projets de l'autre. Bloquer une personne fait de même, et elle ne
peut pas vous envoyer de demandes d'ami. Un terminal partagé s'exécute en votre nom : les
personnes qui y ont accès peuvent aussi exécuter `kodosi` en votre nom et lire et modifier
vos fichiers sur cet ordinateur ; ne le partagez donc qu'avec des personnes à qui vous
confiez cet ordinateur. Kodosi fait confiance à toutes les personnes qui partagent un
terminal ou un projet, ainsi qu'aux programmes et aux agents de leurs terminaux, et ne limite
pas ce qu'ils font. Chacun de vos appareils approuvés contrôle tous vos terminaux et peut en
lancer de nouveaux sur vos autres ordinateurs qui exécutent Kodosi. Depuis n'importe lequel de vos appareils, vous pouvez écrire à Dosi sur vos ordinateurs, et Dosi y agit comme lorsque vous lui écrivez sur l'ordinateur même. Un appareil que vous retirez ne peut plus écrire à Dosi.

Toutes les personnes qui peuvent ouvrir un terminal voient aussi les fichiers de son canvas et
utilisent son navigateur partagé : ne vous y connectez donc que là où toutes ces personnes
peuvent agir. Le navigateur partagé d'un terminal partagé n'utilise jamais vos connexions
enregistrées. Les fichiers d'un canvas sont des fichiers normaux, et toutes les personnes qui
peuvent ouvrir le terminal peuvent les modifier. Une page sur le canvas et un document en
direct s'exécutent dans un cadre fermé, sans réseau, sans fichiers et sans stockage qui
persiste. Tant qu'un navigateur partagé est éveillé, son port de débogage et le port de
pilotage de chaque fenêtre écoutent sur `127.0.0.1`, où d'autres comptes de cet ordinateur
peuvent les atteindre. Un site dans le navigateur partagé enregistre un téléchargement dans le
dossier du terminal pour chaque action d'une personne ou d'un agent, et il n'accède à votre
presse-papiers que lorsque vous copiez. Quand un appareil ouvre le navigateur d'un terminal sur
un autre ordinateur, cet ordinateur écoute sur un port TCP pendant quelques secondes, pour que
les deux se connectent directement. Seul cet appareil passe, avec le chiffrement du terminal.
L'app Linux n'affiche des pages que là où Chromium peut les isoler dans un sandbox ; sous
Ubuntu, son profil AppArmor permet à l'app, à son processus de pages et à Kodosi Browser de
créer les user namespaces dont le sandbox a besoin. Pour la même raison, le navigateur partagé
ne tourne pas en root sous Linux. Les apps ne contiennent pas Kodosi Browser. Quand un
ordinateur ouvre son premier navigateur partagé, son hôte récupère le navigateur dans le flux
de `updates.kodosi.com` et y cherche une nouvelle version chaque jour. L'hôte n'utilise un
téléchargement que si le flux porte la signature de la clé des mises à jour de Kodosi et que le
téléchargement a le SHA-256 indiqué par le flux.

## Ce que voit le service

Le service voit les données des comptes et des appareils, les membres des projets, les
métadonnées des projets et des terminaux, l'agent et l'état de chaque terminal (par exemple en
cours ou en attente d'une approbation, jamais son texte), les champs de routage, les horaires
et la taille des textes chiffrés. Les notifications push vers votre iPhone, iPad ou appareil
Android passent par le service push d'Apple ou de Google, qui voit les identifiants du
terminal et de son ordinateur, l'agent, la place du compte de l'agent et l'état, jamais de
noms ni de texte. Le chiffrement ne cache pas ces métadonnées et n'empêche pas
une panne. Un appareil que vous retirez perd l'accès dès ce moment. Après avoir retiré un
appareil perdu ou volé, créez une nouvelle clé de récupération pour garder cet appareil
dehors. Quand quelqu'un repart de zéro sur votre compte alors que vous avez des appareils,
cela attend un jour et vos appareils l'affichent, pour que vous puissiez l'arrêter. Les
identifiants GitHub et Gitea restent sur votre ordinateur.

Supprimer un compte retire ses appareils, ses données de récupération, ses amis, ses partages
et les projets dont il est propriétaire ; les messages dans les projets des autres restent, sans
compte. Pendant 30 jours, le service garde l'émetteur et le sujet de la connexion supprimée
pour refuser les anciens jetons.

## Vérifier un téléchargement

Chaque version contient `SHA256SUMS`, le SHA-256 de chacun de ses fichiers, et
`SHA256SUMS.asc`, la signature de cette liste par la clé de Kodosi qui signe aussi le dépôt
apt. Placez les deux fichiers à côté de votre téléchargement, puis lancez :

```sh
curl -fsSLO https://updates.kodosi.com/apt/kodosi-archive-keyring.pgp
gpgv --keyring ./kodosi-archive-keyring.pgp SHA256SUMS.asc SHA256SUMS
shasum -a 256 --ignore-missing -c SHA256SUMS
```

`gpgv`, fourni avec GnuPG, doit indiquer une signature valide de la clé
`E8007A36D199B6EE755B1EE9C612C00442E52299`, et `shasum` doit afficher OK pour votre
téléchargement.

## Signaler une vulnérabilité

Utilisez le [signalement privé de vulnérabilités](https://github.com/johnkozaris/Kodosi/security/advisories/new)
ou écrivez à [security@kodosi.com](mailto:security@kodosi.com) avec la révision, les étapes,
l'impact et des logs expurgés. Ne mettez pas de détails dans les issues publiques.
