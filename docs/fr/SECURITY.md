# Sécurité

[English](../SECURITY.md) · [Deutsch](../de/SECURITY.md) · [Ελληνικά](../el/SECURITY.md) · [Español](../es/SECURITY.md) · **Français** · [Italiano](../it/SECURITY.md) · [日本語](../ja/SECURITY.md) · [한국어](../ko/SECURITY.md) · [简体中文](../zh-Hans/SECURITY.md)

Vos appareils chiffrent le trafic des terminaux et le contenu des salons de bout en bout. Les
clés privées restent sur les appareils. Le backend achemine les connexions et stocke les
données chiffrées des salons ; il n'exécute jamais de commandes et n'en voit jamais le texte
en clair. La [conception du chiffrement](../CRYPTOGRAPHY.md) (en anglais) décrit le modèle de
menaces, les mécanismes et les limites connues ; aucun organisme indépendant ne l'a encore
auditée.

## Ce que donne le partage

Partager un terminal donne le contrôle total, avec les privilèges système de l'utilisateur de
l'hôte. Tout le monde dans un salon peut utiliser les terminaux partagés du salon, y compris
les membres invités plus tard. Partager un terminal ne partage rien d'autre. Kodosi fait
confiance à toutes les personnes qui partagent un terminal ou un salon, ainsi qu'aux
programmes et aux agents de leurs terminaux, et ne limite pas ce qu'ils font. Chacun de vos
appareils approuvés contrôle tous vos terminaux et peut en lancer de nouveaux sur vos autres
ordinateurs qui exécutent Kodosi.

Toutes les personnes qui peuvent ouvrir un terminal voient aussi les fichiers de son canvas et
utilisent son navigateur partagé : ne vous y connectez donc que là où toutes ces personnes
peuvent agir. Un terminal partagé n'a jamais vos connexions enregistrées. Les fichiers d'un
canvas sont des fichiers normaux, et toutes les personnes qui peuvent ouvrir le terminal
peuvent les modifier. Une page sur le canvas et un document en direct s'exécutent dans un cadre
fermé, sans réseau, sans fichiers et sans stockage qui persiste. Tant qu'un navigateur partagé
est éveillé, son port de débogage écoute sur `127.0.0.1`, où d'autres comptes de cet
ordinateur peuvent l'atteindre. Quand un appareil ouvre le navigateur d'un terminal sur un
autre ordinateur, cet ordinateur écoute sur un port TCP pendant quelques secondes, pour que les
deux se connectent directement. Seul cet appareil passe, avec le chiffrement du terminal.
L'app Linux n'affiche des pages que là où Chromium peut les isoler dans un sandbox ; sous
Ubuntu, son profil AppArmor permet à l'app et à son processus de pages de créer les user
namespaces dont le sandbox a besoin.

## Ce que voit le service

Le service voit les données des comptes et des appareils, les membres des salons, les
métadonnées des salons et des terminaux, l'agent et l'état de chaque terminal (par exemple en
cours ou en attente d'une approbation, jamais son texte), les champs de routage, les horaires
et la taille des textes chiffrés. Le chiffrement ne cache pas ces métadonnées et n'empêche pas
une panne. Un appareil que vous retirez perd l'accès dès ce moment. Après avoir retiré un
appareil perdu ou volé, créez une nouvelle clé de récupération pour garder cet appareil
dehors. Les identifiants GitHub et Gitea restent sur votre ordinateur.

Supprimer un compte retire ses appareils, ses données de récupération, ses amis, ses partages
et les salons dont il est propriétaire ; les messages dans les salons des autres restent, sans
compte. Pendant 30 jours, le service garde l'émetteur et le sujet de la connexion supprimée
pour refuser les anciens jetons.

## Signaler une vulnérabilité

Utilisez le [signalement privé de vulnérabilités](https://github.com/johnkozaris/Kodosi/security/advisories/new)
ou écrivez à [security@kodosi.com](mailto:security@kodosi.com) avec la révision, les étapes,
l'impact et des logs expurgés. Ne mettez pas de détails dans les issues publiques.
