# Prise en main

[English](../GETTING_STARTED.md) · [Deutsch](../de/GETTING_STARTED.md) · [Ελληνικά](../el/GETTING_STARTED.md) · [Español](../es/GETTING_STARTED.md) · **Français** · [Italiano](../it/GETTING_STARTED.md) · [日本語](../ja/GETTING_STARTED.md) · [한국어](../ko/GETTING_STARTED.md) · [简体中文](../zh-Hans/GETTING_STARTED.md)

[Téléchargez Kodosi](README.md#obtenir-kodosi) pour votre ordinateur. Les raccourcis ci-dessous
utilisent les touches macOS ; sous Linux et Windows, remplacez ⌘ par Ctrl+Maj, sauf mention
contraire.

## 1. Se connecter

Ouvrez Kodosi et cliquez sur **Se connecter** en bas de la barre latérale. Kodosi ouvre votre
navigateur pour vous authentifier. Depuis un terminal, vous pouvez exécuter `kodosi auth login`
pour obtenir un code d'appareil à saisir dans n'importe quel navigateur.

Vous pouvez lancer des terminaux locaux sans vous connecter. Connectez-vous pour relier vos autres
ordinateurs, ajouter des amis et utiliser les projets.

## 2. Lancer un agent

Appuyez sur ⌘N (ou cliquez sur **Nouveau terminal**) pour ouvrir le lanceur. Choisissez un agent et
un dossier, sélectionnez si besoin un compte, un modèle et un niveau d'effort, saisissez un premier
prompt, puis appuyez sur Retour. Kodosi saisit la commande de lancement dans un nouveau shell.

<p align="center"><img src="../media/launcher.webp" width="580" alt="Le lanceur configuré avec Claude Code, un dossier et un premier prompt."></p>

Si un agent n'est pas encore installé, le lanceur affiche sa commande d'installation.

### Ouvrir des terminaux rapidement

- Appuyez sur ⌘D (Ctrl+Maj+O sous Linux et Windows) pour ouvrir un shell simple dans le dossier du
  terminal sélectionné.
- Appuyez sur ⌘T (Ctrl+Maj+T) pour ouvrir un nouveau terminal avec l'agent, le compte et les
  réglages du terminal sélectionné.
- Cliquez sur le bouton `+` à côté d'un dossier dans la barre latérale pour lancer un autre
  terminal avec l'agent qui tourne déjà dans ce dossier, en reprenant le même compte et les mêmes
  réglages (ou un shell simple si aucun agent n'y tourne).
- Dans un terminal inactif au niveau d'une invite de commande, cliquez sur le bouton d'actions
  (deux curseurs) dans l'en-tête pour lancer n'importe quel agent installé avec vos derniers
  réglages utilisés.

### Raccourcis clavier

Les raccourcis reprennent ceux par défaut de Ghostty et sont personnalisables dans **Réglages ›
Raccourcis** :

- ⌘1 à ⌘9 (Alt+1 à Alt+9 sous Linux et Windows) : passer à un terminal selon sa position.
- ⌥⌘ + touches fléchées (Ctrl+Alt + touches fléchées) : passer à un terminal voisin.
- ⌘W : fermer le terminal sélectionné.
- ⌘↑ et ⌘↓ : sauter d'une invite de commande à l'autre dans zsh et dans les versions récentes de
  bash (mais pas dans bash 3.2 fourni avec macOS).

### Autorisations, verbosité et mode rapide

- Par défaut, les agents démarrent sans restriction, afin de pouvoir modifier des fichiers et
  exécuter des commandes sans vous demander confirmation. Antigravity CLI fait exception : il
  démarre dans son propre mode automatique, qui demande confirmation avant toute action
  irréversible. Pour qu'un agent vous demande d'abord l'autorisation, sélectionnez **Demander** dans
  **Plus** (après le sélecteur de modèle).
- Dans **Plus**, vous pouvez aussi régler la verbosité de sortie de Claude Code, Codex, Copilot CLI
  et Antigravity CLI (réglée par défaut sur **Discrète**). Comme Copilot CLI ne lit sa verbosité
  que dans ses réglages enregistrés, Kodosi modifie temporairement la configuration de Copilot
  lorsque Copilot CLI tourne en mode Discrète, puis la rétablit ensuite.
- Claude Code et Codex proposent un mode rapide (réponses plus rapides pour un coût plus élevé),
  activable à côté des sélecteurs de modèle et d'effort. Le mode rapide ne s'applique qu'à ce
  lancement et se désactive automatiquement au lancement suivant.

### Comptes et changement à la volée

- L'ajout d'un nouveau compte lui attribue un dossier de configuration isolé, par exemple
  `~/.claude-work`. Kodosi détecte également les comptes définis via des alias de shell, comme
  `alias cw='CLAUDE_CONFIG_DIR=~/.claude-work claude'`.
- Pendant qu'un agent tourne, utilisez le bouton d'actions (deux curseurs) dans l'en-tête pour
  changer son modèle, son niveau d'effort, son mode rapide et (pour les agents pris en charge) ses
  autorisations sans le relancer. Kodosi envoie la commande slash propre à l'agent dès que le tour
  en cours se termine. Notez que Claude Code mémorise aussi les changements de modèle, d'effort et
  de mode rapide pour les sessions suivantes.

### Liens, images, worktrees et thèmes

Maintenez ⌘ et cliquez sur un lien (Ctrl-clic sous Linux et Windows) pour l'ouvrir dans le
navigateur par défaut de l'appareil que vous utilisez ; maintenez aussi Maj si le programme en
cours capture la souris. Sur iPhone, touchez un lien (désactivé lorsqu'un programme capture la
souris). Kodosi demande confirmation avant d'ouvrir un lien dans une autre app et bloque
l'ouverture de fichiers exécutables ainsi que les chemins de fichiers locaux ou les URL `localhost`
provenant d'ordinateurs distants.

Pour transmettre une capture d'écran ou une image à un agent, collez-la avec ⌘V (Ctrl+Maj+V sous
Linux et Windows, ou Coller sur iPhone). Kodosi l'enregistre sur la machine hôte du terminal et
colle le chemin du fichier pour que l'agent puisse la joindre, en affichant la progression de
l'envoi pour les terminaux distants. Le raccourci de collage propre à l'agent (comme Ctrl+V dans
Claude Code) continue de lire le presse-papiers de la machine sur laquelle l'agent s'exécute.

Pour donner à un agent une copie isolée d'un dépôt Git, survolez le dossier du dépôt dans la barre
latérale, cliquez sur le bouton de branche et saisissez un nom de branche. Kodosi ouvre le terminal
dans un nouveau worktree Git et le supprime automatiquement à la fermeture du terminal si rien n'a
changé. Vous pouvez choisir où sont stockés les worktrees dans **Réglages › Terminal**.

Chaque terminal est clair ou sombre comme Kodosi sur son ordinateur hôte, sur tous les appareils
qui l'affichent. Les programmes à l'intérieur du terminal reçoivent les notifications de thème, de
sorte que des outils comme Claude Code (avec `/theme` réglé sur Auto) s'adaptent automatiquement.
Si vous préférez des terminaux sombres lorsque Kodosi est en mode clair, activez **Garder les
terminaux sombres** dans **Réglages › Terminal**.

### Fermeture, sortie et autorisations macOS

Chaque terminal est un vrai shell qui tourne sur son ordinateur hôte. **Réduire** masque un
terminal tout en le laissant actif ; **Fermer** met fin au shell et à ses processus enfants. Fermer
la fenêtre de l'app laisse Kodosi et vos terminaux actifs dans la barre des menus ou la zone de
notification (sauf sur les bureaux Linux dépourvus de zone de notification, où fermer la fenêtre
quitte Kodosi). Vous pouvez modifier le comportement à la fermeture de la fenêtre dans **Réglages**.

Lorsque vous quittez Kodosi, vos terminaux s'arrêtent et Kodosi enregistre l'état de leur écran
localement sur votre machine (jamais sur le serveur). À la prochaine ouverture de Kodosi, vos
terminaux se rouvrent dans les mêmes dossiers, projets et emplacements sur la scène, avec leur
écran précédent et leur historique de défilement au-dessus d'une nouvelle invite (faites défiler
vers le haut sous Windows pour les voir). Claude Code, Codex et Copilot CLI reprennent
automatiquement leurs conversations, et conservent également sous macOS et Linux le modèle,
l'effort, le mode rapide, les autorisations et la verbosité choisis. Si Kodosi ne parvient pas à
identifier la conversation qu'exécutait un agent, celui-ci affiche la liste de ses conversations
pour vous laisser en sélectionner une.

Sur macOS, les programmes lancés dans Kodosi héritent des autorisations macOS accordées à Kodosi,
comme dans toute autre app de terminal. Si macOS empêche un programme de modifier une autre app,
activez Kodosi dans **Gestion des apps** (accessible depuis **Réglages › Général**). Vous pouvez
également y accorder **Contrôle de l'appareil et accès aux données** (appelé **Accessibilité**
avant macOS 27) et **Enregistrement de l'écran et des sons du système** pour les outils qui
automatisent des apps ou capturent l'écran.

## 3. Voir quel agent a besoin de vous

L'icône de chaque terminal indique son état actuel :

| Indicateur | Signification |
| --- | --- |
| Lumière chaude tournant autour de l'icône | En cours (un anneau qui se remplit indique le pourcentage d'avancement lorsqu'il est signalé). |
| Main, point d'interrogation ou clé | En attente de votre approbation, de votre réponse ou de votre connexion. |
| Coche verte | Terminé. |
| Triangle d'avertissement jaune | Échec. |

La coche et l'avertissement disparaissent dès que vous ouvrez ce terminal sur n'importe lequel de
vos appareils.

Lorsqu'un terminal passe en attente, termine ou échoue, Kodosi vous avertit une seule fois sur
l'appareil que vous utilisez réellement :

- Si Kodosi est au premier plan, vous voyez simplement l'indicateur d'état.
- Si vous utilisez une autre app sur un ordinateur, Kodosi y affiche une notification de bureau.
- Si vous n'êtes devant aucun de vos ordinateurs, Kodosi envoie une notification push sur votre
  téléphone. Si un terminal en attente n'a toujours pas été consulté au moment où vous quittez votre
  ordinateur, la notification est alors relayée sur votre téléphone.

Seuls les terminaux en attente émettent un son de notification. Vous pouvez couper les notifications
par appareil dans **Réglages**. Les programmes signalent leur état à l'aide du
[Program Status Protocol](https://www.superlogical.com/rex/docs/build/program-status) ou des
séquences standard de progression de terminal ; les programmes qui ne remontent pas d'état
n'affichent simplement aucun indicateur.

### Vue Chat

Les terminaux exécutant Claude Code, ainsi que Codex et Copilot CLI lorsqu'ils sont lancés par
Kodosi, proposent également une vue **Chat** dans l'en-tête du terminal. Le chat affiche vos
messages, les réponses et l'activité des outils de l'agent, ainsi que ses questions en attente avec
des boutons cliquables. Vous pouvez répondre à l'agent ou l'arrêter directement depuis le chat ;
si vous saisissez un texte libre pendant qu'il pose une question, cela lui indique quoi faire à la
place. Tout ce que vous faites dans le chat se répercute aussi dans le terminal sous-jacent.

Sur iPhone, iPad et Android, touchez le micro à côté du champ de message pour parler au lieu de
taper. Vos mots s'affichent comme votre message pendant que vous parlez et partent après une longue
pause, pour vous laisser le temps de réfléchir ; touchez **Envoyer** pour les envoyer plus tôt. La
voix est convertie en texte sur votre appareil, et aucun son ne le quitte.

## 4. Voir ce qu'un agent affiche sur le canvas

Les agents peuvent afficher des pages HTML, du Markdown, des images, des PDF, des documents Office,
des feuilles de calcul, des présentations ou des apps web en cours d'exécution sur un **canvas** à
côté du terminal, et piloter un navigateur partagé. Par défaut, le canvas s'ouvre automatiquement dès
que l'agent affiche ou met à jour un élément pendant que vous regardez ce terminal (sauf si vous
l'avez fermé). Pour ne l'ouvrir que vous-même, désactivez **Ouvrir le canvas automatiquement**
dans **Réglages › Général**.

Claude Code, Copilot CLI et Antigravity CLI savent utiliser le canvas même lorsque vous les lancez
manuellement, tandis que Codex, Cursor et OpenCode savent le faire lorsqu'ils sont lancés par
Kodosi (OpenCode sous macOS et Linux). Pour tout autre agent, demandez-lui d'exécuter
`kodosi skill canvas`.

Dès que le canvas contient un élément, l'en-tête du terminal ajoute les boutons **Fichiers** et
**Navigateur**, accompagnés d'un point cuivré en cas de nouveauté :

- Cliquez sur l'un ou l'autre bouton pour ouvrir le canvas à côté du terminal.
- Cliquez sur **Plus d'espace** pour laisser le canvas occuper la scène tout en réduisant le
  terminal dans un coin.
- Fermez un panneau avec la croix `×` de son bouton, et rouvrez-le avec `+`.
- Ouvrez le navigateur manuellement via **Ouvrir le navigateur** dans le menu d'actions, ou ajoutez
  des fichiers via **Ajouter un fichier…** (ou par glisser-déposer sur le canvas), ce qui copie
  également le fichier dans le dossier du terminal.
- Sur iPhone, le bouton du canvas se trouve dans la barre du terminal, et un bandeau signale les
  éléments ajoutés ou mis à jour avec un bouton **Ouvrir**.

### Documents en direct et pages

- Les documents (`.docx`), feuilles de calcul (`.xlsx`), présentations (`.pptx`), PDF et fichiers
  texte fonctionnent en direct : vous, toute personne partageant le terminal et l'agent pouvez les
  modifier simultanément tout en les conservant comme des fichiers ordinaires sur le disque.
  Lorsqu'un document contient des modifications suivies, la barre d'outils propose une commande pour
  les accepter ou les refuser. Dans **Réglages › Documents**, vous pouvez activer ou désactiver
  chaque visionneuse, basculer entre l'affichage paginé et le texte continu, et activer les pages
  sombres en mode sombre.
- Les pages HTML du canvas s'exécutent totalement hors ligne, sans accès au réseau. Les liens dans
  les pages, documents, feuilles de calcul, diapositives et PDF s'ouvrent dans votre propre
  navigateur selon les mêmes règles de sécurité que les liens du terminal (⌘-clic ou Ctrl-clic dans
  les vues modifiables ; clic simple ailleurs ; toucher sur iPhone). Les liens internes vers des
  diapositives ou des cellules y mènent directement.
- Sélectionnez du texte sur une page ou dans un document, puis cliquez sur **Poser une question**
  pour envoyer directement le nom du fichier et l'extrait sélectionné dans le terminal afin d'y taper
  votre question.
- Faites un clic droit sur l'onglet d'un fichier (ou un appui long sur mobile) pour enregistrer une
  copie, retirer ce fichier ou tous les fichiers du canvas, ou déplacer le fichier vers la corbeille
  de l'ordinateur hôte.

### Navigateur partagé

- Cliquez et saisissez du texte dans le navigateur partagé comme dans un navigateur ordinaire.
  L'agent voit les mêmes pages et sessions connectées, et la sortie de sa prochaine commande de
  navigateur lui indique que vous avez utilisé le navigateur.
- Après 10 minutes d'inactivité, le navigateur se met en veille et rouvre sa dernière page dès que
  vous ou l'agent le réutilisez. La toute première fois que vous ouvrez un navigateur partagé sur un
  ordinateur, Kodosi télécharge automatiquement Kodosi Browser, ce qui prend un moment.
- Pour réutiliser vos sessions de navigateur existantes dans vos terminaux privés, cliquez sur
  **Copier** à côté d'un navigateur dans **Réglages › Navigateur**. Cliquez sur **Retirer** pour
  effacer ce que vous avez copié, ou sur **Tout supprimer** pour effacer toutes les connexions et
  données de sites dans vos terminaux privés. Sur macOS, si le système restreint l'accès aux données
  d'un navigateur (comme Safari), cliquez sur **Autoriser…** à côté de ce navigateur pour accorder
  l'accès complet au disque.
- Toute personne ayant accès à un terminal voit son canvas et partage son navigateur. Fermer le
  terminal vide son canvas et supprime les connexions du navigateur du terminal partagé.

## 5. Demander à Dosi

Dosi est l'agent chargé de coordonner vos autres agents. Cliquez sur le visage de Dosi en bas de la
barre latérale (ou appuyez sur ⌘J). Dosi s'appuie sur Claude Code ou Codex, que vous pouvez
configurer avec son compte, son modèle et son niveau d'effort dans **Réglages › Dosi**.

<p align="center">
  <picture>
    <source media="(prefers-reduced-motion: reduce)" srcset="../media/dosi-still.webp">
    <img src="../media/dosi.webp" width="900" alt="Lorsqu'on lui demande ce qui requiert votre attention, Dosi vérifie les terminaux, signale que checkout attend une approbation et demande s'il doit l'autoriser.">
  </picture>
</p>

Demandez à Dosi ce qui requiert votre attention, ou confiez-lui une tâche. Il peut inspecter vos
terminaux et projets, répondre aux demandes, démarrer ou fermer des terminaux et relayer des
messages. Lorsqu'une décision est nécessaire, Dosi affiche des boutons de choix accompagnés d'un
champ pour répondre avec vos propres mots.

Votre conversation avec Dosi reste synchronisée sur tous vos appareils. Sur iPhone et iPad,
choisissez à quel ordinateur adresser votre message juste au-dessus du champ de saisie (par défaut
celui qui a répondu en dernier). Les messages envoyés à un ordinateur hors ligne restent en file
d'attente jusqu'à sa reconnexion, et vous pouvez annuler un message en attente avec **Ne pas
envoyer**.

La ligne d'état à côté du visage de Dosi indique s'il travaille, s'il a répondu ou s'il attend une
réponse. Si aucun agent n'est encore installé sur cet ordinateur, elle affiche **Configurer Dosi** et
ouvre **Réglages › Dosi**. Pour désactiver Dosi sur un ordinateur, désactivez **Utiliser Dosi**
dans **Réglages › Dosi**.

## 6. Ajouter vos autres ordinateurs et téléphones

Connectez-vous sur un nouvel ordinateur avec le même compte. Il affiche un code de vérification, et
vos appareils déjà connectés vous invitent à l'approuver. Cliquez sur **Approuver** sur un
ordinateur déjà approuvé et saisissez le code, ou scannez-le depuis un téléphone approuvé.

Ajouter un téléphone ou une tablette se fait en un seul scan : sur un ordinateur approuvé, ouvrez
**Réglages**, cliquez sur **Afficher un code** sous **Ajouter un appareil**, puis scannez-le avec le
nouvel appareil. Une fois approuvé, les terminaux de tous vos ordinateurs apparaissent partout.

### Lancer des terminaux à distance

Pour lancer un terminal sur un autre ordinateur, sélectionnez cet ordinateur à droite de la barre du
lanceur. Le menu des dossiers liste alors les dossiers récents de cette machine, et **Choisir un
dossier…** permet de parcourir son dossier personnel. Sur iPhone et iPad, touchez `+` dans
**Terminaux** ou à côté d'un ordinateur précis. Kodosi doit être ouvert sur l'ordinateur cible (ou
`kodosi host` doit y tourner dans un terminal).

Lorsque deux de vos appareils peuvent se joindre directement, le trafic des terminaux et du
navigateur circule en pair à pair pour réduire la latence. Sur un réseau local, macOS, iOS et
Android demandent une fois l'autorisation d'accès au réseau local ; si vous la refusez, le trafic
passe par le serveur relais.

### Mise en veille et maintien en éveil

Un ordinateur ne se met pas en veille de lui-même pendant qu'un agent travaille dans l'un de ses
terminaux, et il peut de nouveau se mettre en veille quelques minutes après l'arrêt de ses agents.
Un agent qui attend votre réponse ne le garde pas éveillé. Pour changer cela, choisissez une option
pour **Rester éveillé** dans **Réglages › Général** sur cet ordinateur : **Pendant le travail des
agents**, **Toujours** ou **Jamais**. **Toujours** ne garde un ordinateur éveillé que lorsqu'il est
branché ; sur batterie, il reste éveillé pendant le travail des agents. Vous pouvez toujours mettre
un ordinateur en veille vous-même.

Pour mettre en veille un autre de vos ordinateurs, ouvrez **Réglages**, cliquez sur **⋯** à côté de
lui, sous **Vos appareils**, puis choisissez **Mettre en veille…**. Sur un téléphone, touchez
l'ordinateur pour ouvrir sa page. Ses terminaux sont en pause jusqu'à son réveil. Si rien ne pouvait
le réveiller ensuite, Kodosi vous le dit avant. Le même menu, ou la page, choisit quand cet
ordinateur reste éveillé.

### Réveiller un ordinateur

Pour réveiller un ordinateur en veille, cliquez sur **Réveiller** à côté de lui dans **Réglages**
(sur un téléphone, sur sa page), sur l'un de ses terminaux ou dans la liste des ordinateurs d'un
nouveau terminal. Le signal de réveil doit partir du réseau de l'ordinateur en veille. Kodosi
l'envoie depuis l'appareil que vous utilisez lorsqu'il est sur le même réseau, et sinon depuis un
autre de vos ordinateurs éveillés sur ce réseau. Un iPhone ou un iPad le demande toujours à l'un de
vos ordinateurs.

Un ordinateur se réveille par un câble réseau, pas en Wi-Fi, et seulement si son système
l'autorise. **Réglages** indique pour chaque ordinateur ce que Kodosi attend, ou le seul réglage à
modifier :

- Sur un Mac, activez **Réactiver lors des accès réseau** dans Réglages Système.
- Sous Windows, autorisez la carte réseau à sortir l'ordinateur de veille, dans le Gestionnaire de
  périphériques.
- Sous Linux, activez Wake-on-LAN pour la connexion filaire.

Un Mac n'envoie le signal de réveil que si Kodosi peut utiliser le réseau local. Autorisez-le dans
Réglages Système, sous **Confidentialité et sécurité**.

Pour en être sûr, choisissez **Tester le réveil…** dans le menu d'un ordinateur éveillé (sur un
téléphone, sur sa page). Kodosi le met en veille, le réveille et garde le résultat. Restez près de
l'ordinateur pour le premier test : s'il ne se réveille pas, réveillez-le à la main. Un PC Windows
avec Modern Standby n'a pas **Tester le réveil…**, car Windows ne montre pas aux apps quand il se
met en veille.

### Clés de récupération

Créez une clé de récupération dans **Réglages** et conservez-la en lieu sûr. Si vous perdez tous
vos appareils, connectez-vous sur un nouvel appareil et sélectionnez **« Utiliser ma clé de
récupération »** pour conserver vos amis et vos projets. Supprimez les appareils perdus depuis l'un
de vos appareils restants ; si un appareil a été volé, générez également une nouvelle clé de
récupération ensuite.

## 7. Travailler avec des amis

Ouvrez **Amis** depuis le menu à côté de votre compte. Envoyez votre invitation, ou collez
l'invitation ou le nom d'utilisateur d'un ami. Passer par une invitation permet à Kodosi de vérifier
automatiquement l'identité de votre ami. Sur mobile, vous pouvez afficher votre invitation sous
forme de QR code qu'un ami scanne avec son appareil photo ou depuis Kodosi ; il confirme tout de
même sa confiance avant l'envoi de la demande.

<p align="center"><img src="../media/friends.webp" width="900" alt="La page Amis affichant deux amis vérifiés."></p>

### Partager un terminal

Ouvrez le menu d'actions dans l'en-tête d'un terminal, cliquez sur **Partager**, puis sélectionnez
des amis ou des projets. Toute personne avec qui vous le partagez en obtient le contrôle total, avec
vos autorisations système sur cette machine. Vos autres terminaux restent privés. Vous pouvez
partager depuis n'importe lequel de vos appareils, y compris votre téléphone, tant que l'ordinateur
qui exécute le terminal est en ligne. Kodosi vous demande de confirmer chaque fois que quelqu'un de
nouveau obtient l'accès.

### Projets

Un projet rassemble des terminaux issus de n'importe quel dossier ou ordinateur, accompagnés d'un
chat partagé et d'une liste de tâches. Cliquez sur `+` à côté de **Projets**, lancez des terminaux
dans le projet (ou partagez-y des terminaux existants), puis invitez des amis dès que vous êtes
prêt.

<p align="center"><img src="../media/project.webp" width="900" alt="Un projet réunissant un terminal partagé et une conversation entre personnes et agents."></p>

- Chaque membre peut utiliser les terminaux partagés et partager les siens. Les personnes qui
  rejoignent le projet plus tard voient immédiatement ses terminaux et l'intégralité de l'historique
  du chat. Les fichiers restent sur l'ordinateur hôte de chaque terminal.
- Créez des tâches disponibles pour tous, prenez-les en charge, libérez-les, clôturez-les avec une
  note de synthèse ou rouvrez-les.
- Liez des dépôts GitHub ou Gitea pour synchroniser les issues avec les tâches : modifier une tâche
  liée met à jour l'issue correspondante. GitHub utilise votre connexion `gh` ou le gestionnaire
  d'identifiants Git ; Gitea utilise le gestionnaire d'identifiants Git ou `KODOSI_GITEA_TOKEN`
  avec `KODOSI_GITEA_URL`. Vos identifiants ne quittent jamais votre machine.
- Mentionnez un terminal dans le chat avec `@<terminal-name>` pour transmettre votre message
  (accompagné de votre nom et du projet) directement à l'agent qui y tourne ; l'agent décide comment
  répondre. Claude Code, Copilot CLI et Antigravity CLI dans Kodosi (ainsi que Codex, Cursor et
  OpenCode lorsqu'ils sont lancés par Kodosi, OpenCode étant pris en charge sous macOS et Linux)
  savent automatiquement qu'ils tournent dans Kodosi. Exécutez `kodosi skill` pour afficher le
  [skill](../../runtime/skills/kodosi/SKILL.md) destiné à tout autre agent. Dans un terminal
  Kodosi, les commandes de projet détectent automatiquement le projet en cours ; en dehors, passez
  `--project <name>`.
- Supprimer un message envoyé par vous ou votre agent le remplace pour tout le monde par une
  mention indiquant qu'il a été supprimé. **Effacer le chat** masque l'historique jusqu'à ce point
  sur tous vos appareils tout en le laissant intact pour les autres membres.

## 8. Utiliser la ligne de commande

La CLI `kodosi` pilote exactement les mêmes terminaux que l'app de bureau, et chaque action apparaît
immédiatement dans l'app.

| Commande | Ce qu'elle fait |
| --- | --- |
| `kodosi terminal start --agent claude --prompt "fix the failing test"` | Lance un agent avec vos derniers réglages utilisés. Modifiez-les avec `--account`, `--model`, `--effort`, `--fast`, `--permissions` et `--output`. |
| `kodosi terminal start --directory <repository> --branch <name>` | Démarre un terminal sur une nouvelle branche, dans son propre worktree Git. |
| `kodosi terminal start --computer <computer> --directory <folder>` | Démarre un terminal sur un autre de vos ordinateurs. Les chemins relatifs et `~` sont résolus par rapport à votre dossier personnel sur cette machine. Utilisez `kodosi devices list` pour voir quels ordinateurs sont en ligne. |
| `kodosi agent list` | Liste les agents installés, leurs comptes et vos derniers réglages utilisés. |
| `kodosi agent usage` | Affiche l'utilisation actuelle du forfait et les horaires de réinitialisation pour les comptes Claude Code, Codex et Copilot CLI connectés. |
| `kodosi terminal list` | Liste les terminaux avec leurs noms et leurs identifiants (passez l'un ou l'autre comme `<terminal>`). |
| `kodosi terminal read <terminal>` | Affiche le contenu actuel de l'écran. Ajoutez `--history` pour inclure l'historique de défilement. |
| `kodosi terminal message <terminal> "Can you review my branch?"` | Envoie un message à l'agent d'un terminal, accompagné de l'expéditeur et de la provenance. |
| `kodosi terminal agent <terminal> --model sonnet` | Change le modèle, le niveau d'effort, le mode rapide ou les autorisations d'un agent en cours d'exécution. |
| `kodosi terminal input <terminal> --enter "make test"` | Saisit du texte dans un terminal. |
| `kodosi terminal keys <terminal> ctrl-c` | Envoie des frappes de touches à un terminal. |
| `kodosi terminal wait <terminal> --until done,failed` | Attend que le terminal passe dans l'un des états indiqués. |
| `kodosi terminal show <terminal>` | Affiche le terminal au premier plan dans l'app. |
| `kodosi dosi say "What needs me?"` | Envoie un message à Dosi. Utilisez `kodosi dosi show` pour afficher la conversation et `kodosi dosi stop` pour annuler un tour. |
| `kodosi show <file>` | Ouvre un fichier sur le canvas de ce terminal, qui s'ouvre pour les personnes qui regardent le terminal. Utilisez `kodosi show <address>` pour ouvrir une URL dans son navigateur partagé, `kodosi show browser` pour afficher le navigateur tel quel, `kodosi canvas` pour lister les éléments du canvas et `kodosi canvas remove <file>` pour en retirer un. |
| `kodosi browser open <address>` | Ouvre une URL dans le navigateur partagé de ce terminal (toutes les autres commandes d'agent-browser fonctionnent aussi). |
| `kodosi skill` | Affiche le skill permettant à un agent de collaborer dans Kodosi ; `kodosi skill canvas` affiche le skill du canvas. |
| `kodosi dosi skill` | Affiche le skill permettant à un autre agent d'agir comme Dosi. |
| `kodosi devices sleep <computer>` | Met en veille un autre de vos ordinateurs. Ses terminaux sont en pause jusqu'à son réveil. `kodosi devices keep-awake` avec `agents`, `always` ou `never` choisit quand cet ordinateur reste éveillé. |
| `kodosi devices wake <computer>` | Réveille l'un de vos ordinateurs en veille et attend qu'il soit en ligne. `kodosi devices test-wake <computer>` met un ordinateur en veille, le réveille et garde le résultat. `kodosi devices list` indique ce que Kodosi attend de chaque ordinateur. |
| `kodosi host` | Exécute l'hôte de façon indépendante afin que vos terminaux restent actifs après avoir quitté l'app. Utilisez `kodosi host stop` pour arrêter un hôte en arrière-plan. |

`message`, `input` et `keys` refusent de s'exécuter pendant que vous tapez activement dans ce
terminal. `message` refuse également lorsque l'agent attend une réponse humaine.

## Si quelque chose ne se connecte pas

- Vérifiez que l'ordinateur hôte du terminal est en ligne avec Kodosi ouvert, et que le terminal est
  toujours partagé.
- Un appareil récemment ajouté est peut-être encore en attente d'approbation, ou un ami dont
  l'identité a changé doit peut-être être vérifié à nouveau dans **Amis**.
- Après une perte de connexion, Kodosi affiche de nouveau l'état actuel. Si un terminal distant
  affiche **Hors ligne**, cet appareil ne parvient pas à joindre le serveur ; vérifiez votre
  connexion réseau. Kodosi se reconnecte automatiquement, ou cliquez sur **Réessayer** pour relancer
  immédiatement la connexion.
- Lorsqu'un ordinateur hôte passe hors ligne, ses terminaux se reconnectent automatiquement à son
  retour (sauf s'ils ont été fermés entre-temps). Pour retirer un terminal hors ligne de votre
  liste, cliquez sur sa croix `×` : **Masquer jusqu'à son retour** le masque temporairement, et
  **Fermer définitivement** le ferme pour de bon dès que l'ordinateur se reconnecte.
- Si Kodosi avertit que la remise de votre saisie est incertaine, vérifiez l'écran du terminal avant
  de relancer la commande.
