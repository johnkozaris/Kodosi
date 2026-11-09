<p align="center">
  <a href="https://kodosi.com"><img src="../media/banner.webp" width="800" alt="'terminals, agents and friends. together.'라는 문구와 터미널 커서 위에 있는 Kodosi 로고."></a>
</p>

<p align="center">
  <a href="../../README.md">English</a> · <a href="../de/README.md">Deutsch</a> · <a href="../el/README.md">Ελληνικά</a> · <a href="../es/README.md">Español</a> · <a href="../fr/README.md">Français</a> · <a href="../it/README.md">Italiano</a> · <a href="../ja/README.md">日本語</a> · <b>한국어</b> · <a href="../zh-Hans/README.md">简体中文</a>
</p>

<p align="center">
  <a href="../../LICENSE"><img src="https://img.shields.io/badge/license-Apache_2.0-c98566?style=flat-square&labelColor=2a211d" alt="Apache License 2.0"></a>
  <a href="#kodosi-받기"><img src="https://img.shields.io/badge/macOS-26.4%2B%20%C2%B7%20Apple%20silicon-c98566?style=flat-square&labelColor=2a211d" alt="Apple silicon의 macOS 26.4 이상"></a>
  <a href="#kodosi-받기"><img src="https://img.shields.io/badge/Linux-x86--64-c98566?style=flat-square&labelColor=2a211d" alt="Linux x86-64"></a>
  <a href="#kodosi-받기"><img src="https://img.shields.io/badge/Windows-11%2024H2%2B%20%C2%B7%20x64-c98566?style=flat-square&labelColor=2a211d" alt="x64의 Windows 11 24H2 이상"></a>
  <a href="../../clients/ios/README.md"><img src="https://img.shields.io/badge/iOS-26%2B%20%C2%B7%20iPhone%20and%20iPad-c98566?style=flat-square&labelColor=2a211d" alt="iPhone과 iPad의 iOS 26 이상"></a>
  <a href="https://github.com/johnkozaris/Kodosi/releases/latest"><img src="https://img.shields.io/github/v/release/johnkozaris/Kodosi?style=flat-square&labelColor=2a211d&color=c98566&label=release" alt="최신 릴리스"></a>
</p>

[Kodosi](https://kodosi.com)는 Mac, Linux, Windows에서 쓰는 코딩 에이전트용 터미널 앱이에요.
모든 컴퓨터의 터미널을 한 창에 모아 두고, 어떤 에이전트가 나를 기다리는지 알려 주고, 친구들이 같은 터미널에서 함께 일할 수
있게 해 줘요. 터미널마다 옆에 캔버스가 있어서, 에이전트가 계획, 페이지, 문서를 보여 주고 함께 편집할 수 있어요. 터미널과
방은 종단 간 암호화돼요.

<p align="center">
  <picture>
    <source media="(prefers-color-scheme: light)" srcset="../media/app-light.webp">
    <img src="../media/app-dark.webp" width="900" alt="Mac용 Kodosi 앱: Codex가 작업하는 동안 Claude Code가 승인을 기다리고 있어요.">
  </picture>
</p>

## Kodosi로 할 수 있는 것

**모든 터미널을 한 창에.** Mac, Linux나 Windows 컴퓨터, 친구들 컴퓨터의 터미널이 하나의
사이드바에 모여요. 각각은 자기 컴퓨터에서 도는 진짜 셸이고, 같은 창에서 내 어느 컴퓨터에나
새 터미널을 시작할 수 있어요.

**어떤 에이전트가 나를 기다리는지 한눈에.** Claude Code, Codex, Copilot CLI 등 터미널에서
도는 에이전트라면 무엇이든 쓸 수 있어요. 각 터미널은 에이전트가 작업 중인지, 승인을
기다리는지, 끝났는지, 실패했는지 보여 주고, Kodosi가 백그라운드에 있을 때는 알림으로
알려 줘요.

**Dosi, 에이전트를 위한 에이전트.** 무엇이 나를 기다리는지 Dosi에게 물어보세요. Dosi는
모든 터미널과 방을 읽고 알려 주고, 나 대신 움직여요. 터미널에 답을 입력하고, 에이전트를
시작하고, 끝난 것을 닫아요. Dosi와의 대화는 어느 컴퓨터에서든 이어져요.

<p align="center">
  <picture>
    <source media="(prefers-reduced-motion: reduce)" srcset="../media/dosi-still.webp">
    <img src="../media/dosi.webp" width="900" alt="무엇이 나를 기다리는지 묻자, Dosi가 터미널을 읽고 checkout이 승인을 기다린다고 알려 주며 허용할지 묻고 있어요.">
  </picture>
</p>

**실행 중인 터미널을 공유.** 실행 중인 터미널과 그 에이전트의 전체 권한을 친구에게 줄 수
있어요.

**터미널마다 옆에 캔버스.** 에이전트가 터미널 옆에 페이지, 계획, 이미지를 보여 줘요. 문서,
시트, 프레젠테이션, PDF, 텍스트 파일은 실시간이라 나와 친구와 에이전트가 같은 실제 파일을
동시에 편집하고, 에이전트가 바꾼 내용은 수락하거나 거부할 수 있어요. 에이전트는 나와 함께 쓰는
브라우저도 써요. 에이전트가 하는 일을 실시간 영상으로 보고, 같은 페이지에서 직접 클릭하고 입력할
수 있어요.

<p align="center">
  <picture>
    <source media="(prefers-reduced-motion: reduce)" srcset="../media/canvas-still.webp">
    <img src="../media/canvas.webp" width="900" alt="Claude Code 터미널과 캔버스. Leo가 편집하는 출시 계획에 Claude가 변경을 제안하고, 이어서 예산 시트, 슬라이드, PDF, 그리고 Claude가 머그잔을 장바구니에 담는 공유 브라우저가 보여요.">
  </picture>
</p>

**사람과 에이전트를 위한 방.** 방에는 공유 터미널, 대화, 작업이 있어요. 에이전트는 대화를
읽고 글을 올리고 작업을 맡아요. GitHub나 Gitea 이슈는 작업이 돼요.

<p align="center"><img src="../media/room.webp" width="900" alt="공유 터미널과 사람과 에이전트의 대화가 있는 방."></p>

**에이전트마다 브랜치 하나.** 에이전트를 전용 Git worktree의 새 브랜치에서 시작하면 두
에이전트가 같은 파일을 바꾸는 일이 없어요.

**iPhone과 iPad에서도.** 컴퓨터에서 떨어져 있을 때도 터미널과 방을 지켜보고, 답하고,
조작할 수 있어요.

**종단 간 암호화, 양자 내성.** 키는 내 기기를 떠나지 않아요. 기기는 터미널과 방을
ML-KEM-768과 X25519를 함께 써서 종단 간 암호화하므로 서비스는 내용을 읽을 수 없어요.
[보안](SECURITY.md)

## Kodosi 받기

<p>
  <a href="https://updates.kodosi.com/download/macos"><img src="https://img.shields.io/badge/%EB%8B%A4%EC%9A%B4%EB%A1%9C%EB%93%9C-Mac-c98566?style=for-the-badge&labelColor=2a211d" alt="Mac용 Kodosi 다운로드"></a>
  <a href="https://updates.kodosi.com/download/linux"><img src="https://img.shields.io/badge/%EB%8B%A4%EC%9A%B4%EB%A1%9C%EB%93%9C-Linux-c98566?style=for-the-badge&labelColor=2a211d" alt="Linux용 Kodosi 다운로드"></a>
  <a href="https://updates.kodosi.com/download/windows"><img src="https://img.shields.io/badge/%EB%8B%A4%EC%9A%B4%EB%A1%9C%EB%93%9C-Windows-c98566?style=for-the-badge&labelColor=2a211d" alt="Windows용 Kodosi 다운로드"></a>
</p>

- **Mac**(Apple silicon, macOS 26.4 이상): 디스크 이미지를 열고 Kodosi를 응용 프로그램으로
  옮기거나 `brew install --cask johnkozaris/tap/kodosi`를 실행하세요. 앱은 스스로
  업데이트돼요.
- **Linux**(x86-64, Ubuntu 24.04 또는 Debian 13 이상): Debian 패키지를 설치하세요. Kodosi apt
  저장소가 추가되므로 시스템을 업데이트하면 Kodosi도 업데이트돼요.
  [릴리스](https://github.com/johnkozaris/Kodosi/releases/latest)에는 Arch 패키지와 포터블
  압축 파일도 있어요.
- **Windows**(x64, Windows 11 24H2 이상): 다운로드한 파일을 처음 열기 전에 관리자로
  [Kodosi 인증서](https://updates.kodosi.com/windows/Kodosi.cer)를 로컬 컴퓨터 › 신뢰할 수
  있는 사람에 추가하세요. 앱은 스스로 업데이트돼요.
- **iPhone과 iPad:** [iPhone이나 iPad](../../clients/ios/README.md)용으로 Kodosi를 빌드하세요.

그다음 [시작하기](GETTING_STARTED.md)를 따라 하세요.

## 더 알아보기

- [시작하기](GETTING_STARTED.md)
- [보안](SECURITY.md)과 [암호화 설계](../CRYPTOGRAPHY.md)(영어)
- [아키텍처](../ARCHITECTURE.md), [개발](../DEVELOPMENT.md),
  [기여하기](../../CONTRIBUTING.md)(영어)
- [직접 호스팅하기](../SELF-HOSTING.md)(영어)

버그는 [GitHub 이슈](https://github.com/johnkozaris/Kodosi/issues)에, 취약점은
[비공개로](SECURITY.md#취약점-신고) 알려 주세요.

Kodosi는 [John Kozaris](https://johnkozaris.com)가 만들어요.

## 라이선스

Kodosi는 [Apache License 2.0](../../LICENSE)을 따라요. Kodosi의 이름과 로고는 이 라이선스에
포함되지 않아요. 사용 방법은 [상표](../../TRADEMARKS.md)(영어)에서 확인하세요. Kodosi는
[Ghostty](https://ghostty.org), Rust, Swift, Qt, ASP.NET Core를 사용해요. 포함된 항목은
[서드파티 고지](../../terminal/ghostty/THIRD_PARTY_NOTICES.md)와
[이미지 출처](../media/README.md)에 나와 있어요.
