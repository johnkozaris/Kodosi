# 快速上手

[English](../GETTING_STARTED.md) · [Deutsch](../de/GETTING_STARTED.md) · [Ελληνικά](../el/GETTING_STARTED.md) · [Español](../es/GETTING_STARTED.md) · [Français](../fr/GETTING_STARTED.md) · [Italiano](../it/GETTING_STARTED.md) · [日本語](../ja/GETTING_STARTED.md) · [한국어](../ko/GETTING_STARTED.md) · **简体中文**

为你的电脑[获取 Kodosi](README.md#获取-kodosi)。本指南使用 Mac 的按键；在 Linux 和 Windows 上，请用 Ctrl+Shift 加同一个字母。

## 1. 登录

打开 Kodosi，选择侧边栏底部的“登录”。Kodosi 会打开你的浏览器，你在浏览器中登录。在终端中，`kodosi auth login` 会显示一个代码，可在任意浏览器中输入。

没有账号时，Kodosi 也能在这台电脑上运行终端。登录后即可使用你的其他电脑、好友和项目。

## 2. 启动 Agent

选择“新建终端”（⌘N）。选择 Agent 和文件夹，如有需要，还可以选择账号、模型和推理强度。输入第一条提示词并按 Return 键，Kodosi 会把命令输入到新终端的 shell 中。

<p align="center"><img src="../media/launcher.webp" width="580" alt="在启动器中选择了 Claude Code，并填写了文件夹和第一条提示词。"></p>

- 想要普通终端，按 ⌘D（Linux 和 Windows 上为 Ctrl+Shift+O）。它会立即在所选终端的文件夹中打开。⌘T（Ctrl+Shift+T）会用所选终端的 Agent 打开它，账户和选择都相同。
- 按键与 Ghostty 一致。⌘1 到 ⌘9 选择终端（Linux 和 Windows 上为 Alt+1 到 Alt+9），⌥⌘ 加方向键选择相邻的终端（Ctrl+Alt 加方向键），⌘W 关闭所选终端。“设置 › 快捷键”列出了所有按键，并可以修改。
- 如果缺少某个 Agent，启动器会显示它的安装命令。
- Agent 启动时为“不受限制”：会直接修改文件、运行命令，不会先询问你。若想让某个 Agent 先询问，在模型后面的“更多”中选择“询问”。在那里还可以选择“详细程度”，即 Claude Code、Codex 和 Copilot CLI 显示多少工作内容，开始时为“简洁”。Copilot CLI 只从自己的设置中读取详细程度，因此在 Copilot CLI 以“简洁”运行期间，Kodosi 会修改它的设置，结束后再改回来。
- 新账号有自己的登录文件夹，例如 `~/.claude-work`。Kodosi 也会找到你在 shell 中设置的账号，例如 `alias cw='CLAUDE_CONFIG_DIR=~/.claude-work claude'`。
- 在停在提示符处的终端里，标题栏中的操作按钮（两个滑块）会列出已安装的 Agent。选择一个，即可按你上次的选择启动它。
- Agent 运行时，同一个操作按钮可以用 Agent 自己的命令更改它的模型和推理强度，部分 Agent 还能更改权限，无需重新启动。更改会等到 Agent 这一轮结束后生效；Claude Code 还会在之后的会话中沿用这个模型和推理强度。
- 在 zsh 和 bash（不包括 macOS 自带的 bash 3.2）中，⌘↑ 和 ⌘↓ 可在提示符之间跳转。
- 按住 ⌘ 点按链接即可打开（Linux 和 Windows 上为 Ctrl+点按）。在使用鼠标的程序中，还要同时按住 Shift。在 iPhone 上，轻点链接即可；在使用鼠标的程序中，轻点不会打开链接。链接会在你当前所用设备上的浏览器中打开。在其他应用中打开链接之前，Kodosi 会先询问。它不会打开可以运行程序的文件，也不会打开其他电脑上终端的文件或 `localhost` 地址。
- 要把截图等图片交给 Agent，用 ⌘V 粘贴即可（Linux 和 Windows 上为 Ctrl+Shift+V，iPhone 上为“粘贴”）。Kodosi 会把图片保存到终端所在的电脑上，并粘贴文件路径，Agent 会把它作为附件。其他电脑上的终端会显示图片已传输了多少。Agent 自己的图片快捷键（例如 Claude Code 中的 Ctrl+V）读取的是 Agent 所在电脑的剪贴板。
- 要给 Agent 一份独立的仓库副本，在侧边栏中将指针悬停在仓库文件夹上，选择分支按钮并为分支命名。Kodosi 会在新的 Git worktree 中启动终端。终端结束时，如果没有任何改动，Kodosi 会删除这个 worktree。这些文件夹的位置可在“设置 › 终端”中设置。

终端是运行它的那台电脑上的真实 shell。“最小化”会隐藏终端，但它继续运行；“关闭”会结束终端及其中的程序。关闭窗口后，Kodosi 和你的终端会继续运行，Kodosi 图标会留在菜单栏或系统托盘中；在没有系统托盘的桌面环境中，关闭窗口会退出 Kodosi。退出 Kodosi 后，你的终端会停止。Kodosi 只把终端显示的内容保存在你的电脑上，不会保存到别处。再次打开 Kodosi 时，终端会在原来的文件夹、项目和舞台位置重新启动，新提示符上方会显示之前的屏幕内容和历史记录（在 Windows 上，向上滚动即可看到）。Claude Code、Codex 和 Copilot CLI 会继续各自的对话，在 Mac 和 Linux 上还会沿用你选的模型、推理强度、权限和输出。如果 Kodosi 无法判断 Agent 显示的是哪个对话，Agent 会列出它的对话，方便你继续其中一个。你可以在“设置”中选择关闭窗口时的行为。

在 Mac 上，和任何终端 App 一样，终端中的程序拥有你授予 Kodosi 的权限。当 macOS 提示已阻止 Kodosi 修改 App 时，说明终端中的某个程序尝试更改另一个 App。若要允许，请在“App 管理”中打开 Kodosi；“设置 › 常规”可以打开它。你也可以在那里为控制 App 或查看屏幕的程序允许“设备控制和数据访问”（macOS 27 之前为“辅助功能”）和“录屏与系统录音”。

## 3. 看看哪个 Agent 需要你

每个终端的标记显示其程序正在做什么。

| 你看到的 | 含义 |
| --- | --- |
| 一道暖光绕着标记转动 | 工作中。边框会逐渐填满，显示百分比。 |
| 一只手、一个问号或一把钥匙 | 正在等待你的批准、回答或登录。 |
| 一个绿色对勾 | 已完成。 |
| 一个黄色警告标志 | 已失败。 |

打开终端后，对勾和警告就会消失。Kodosi 在后台时，如果你的某个终端开始等待、完成或失败，通知会提醒你。程序通过 [Program Status Protocol](https://www.superlogical.com/rex/docs/build/program-status) 或进度条报告状态。不报告任何内容的程序不会显示状态。

## 4. 查看 Agent 展示的内容

Agent 可以在其终端的画布上给你展示页面、Markdown、图片、PDF、文档、表格、演示文稿或它运行的应用，也可以使用与你共享的浏览器。当你查看它的终端时，画布会随 Agent 展示的内容打开；画布上的文件或浏览器有变化时也会打开，除非你已将它关闭。若只想自己打开画布，请在“设置 › 常规”中关闭“自动打开画布”。Claude Code 和 Copilot CLI 即使由你自己启动也知道该怎么做，由 Kodosi 启动的 Codex 也一样；对于其他 Agent，让它读取 `kodosi skill canvas` 即可。画布上有内容时，终端标题栏会显示文件按钮和浏览器按钮，有新内容时会带一个铜色圆点。选择其中一个，即可在终端旁打开画布；“更多空间”会把舞台交给画布，终端则缩小在角落里。用按钮上的 × 关闭某一部分，用 + 再次打开。操作按钮中的“打开浏览器”会在 Agent 使用之前打开浏览器，“添加文件…”会把文件放到画布上并放入终端的文件夹；你也可以把文件拖放到画布上。在 iPhone 上，画布按钮位于终端的栏中；Agent 展示或更改内容时，会出现一条横幅，点按“打开”即可查看。

- 文档、表格、演示文稿、PDF 或文本文件都是实时的：你、终端里的人和 Agent 可以同时编辑，它仍然是其文件夹中的普通文件。文档有修订记录时，它的栏中会出现一个接受或拒绝修订的按钮。“设置 › 文档”可以开启或关闭每种查看器，选择以页面还是以适应画布的文本显示文档，并在 Kodosi 为深色时让页面也变暗。
- 页面不联网。页面、文档、表格、演示文稿和 PDF 中的链接会按与终端链接相同的规则，在你自己的浏览器中打开。在可以编辑的地方，按住 ⌘ 点按链接（Linux 和 Windows 上为 Ctrl+点按）；在其他地方，点按即可打开，在 iPhone 上轻点即可。指向同一文件中某张幻灯片或某个单元格的链接会直接跳转过去。
- 在页面或文档中选中文字，然后选择“就此提问”。Kodosi 会把文件和这些文字输入到终端中，你再写下问题即可。
- 在共享浏览器中，像在自己的浏览器里一样点按和输入。Agent 能看到相同的页面和登录状态，它的下一条命令会告诉它你用过浏览器。闲置 10 分钟后浏览器会休眠，当你查看它或 Agent 使用它时，它会重新打开最后一个页面。电脑上的第一个浏览器需要下载 Kodosi Browser，所以要稍等片刻。
- 要在你的私有终端中使用你的登录信息，请在“设置 › 浏览器”中选择某个浏览器旁边的“复制”。“移除”会去掉你复制的内容，“全部移除”会删除你私有终端中的所有登录信息和网站数据。在 Mac 上，如果 macOS 像保护 Safari 一样不让 Kodosi 读取某个浏览器的登录信息，请选择该浏览器旁边的“允许…”，为 Kodosi 开启“完全磁盘访问权限”。
- 在同一网络中，浏览器画面会直接在你的设备之间传输。macOS 和 iOS 会询问一次本地网络访问权限；如果不允许，画面会经由服务传输。
- 所有能打开该终端的人都能看到它的画布并使用它的浏览器。终端关闭后，它的画布和浏览器中的登录信息都会消失。

## 5. 问问 Dosi

Dosi 是帮你管理其他 Agent 的 Agent。选择侧边栏底部 Dosi 的头像（⌘J）。Dosi 基于 Claude Code 或 Codex 运行；在“设置 › Dosi”中可以选择用哪一个，以及它的账号、模型和推理强度。

<p align="center">
  <picture>
    <source media="(prefers-reduced-motion: reduce)" srcset="../media/dosi-still.webp">
    <img src="../media/dosi.webp" width="900" alt="被问到什么需要你处理时，Dosi 读取终端，告诉你 checkout 在等待批准，并询问是否允许。">
  </picture>
</p>

问问 Dosi 什么需要你处理，或者让它帮你做点事。Dosi 会读取你的终端和项目，给出回答，并替你输入、启动和关闭各种东西。需要你做选择时，它会用按钮来问，你也可以用自己的话回答。你和 Dosi 的对话在你所有的设备上都是同一个。在 iPhone 或 iPad 上，“Dosi”标签页会把你写的内容发给你在消息上方选择的那台电脑上的 Dosi，起初是最后一个回答的那台。发给离线电脑的消息会一直等到它再次运行 Kodosi，选择“不发送”即可撤回。

Dosi 头像旁的那一行会告诉你 Dosi 何时在工作、何时已回答、何时有问题。在没有 Agent 的电脑上，这里显示“设置 Dosi”，并会打开“设置 › Dosi”。要从某台电脑上移除 Dosi，请在“设置 › Dosi”中关闭“使用 Dosi”。

## 6. 添加你的其他电脑

在新电脑上用同一个账号登录。新电脑会显示一个代码，你已经在用的电脑会请求你批准它。在其中一台上选择“批准”并输入代码，或者用你已在使用的手机上的 Kodosi 扫描这个代码。

新手机或平板只需扫描一次即可加入。在你已在使用的电脑上打开“设置”，在“添加设备”下选择“显示二维码”，然后用新手机扫描。之后，每台电脑都会显示你所有电脑上的终端。

要在你的另一台电脑上启动终端，请在启动器栏的最右侧选择那台电脑。此时文件夹按钮会列出它最近使用的文件夹，“选择文件夹…”会打开它主目录中的文件夹。在 iPhone 或 iPad 上，选择“终端”中的 +，或你的某台电脑旁的 +。那台电脑上需要打开 Kodosi，或者在某个终端中运行 `kodosi host`。

也请在那里生成一个恢复密钥并妥善保管。如果你丢失了所有设备，在新设备上登录并选择“使用我的恢复密钥”，即可保留你的好友和项目。请从另一台设备上移除丢失的设备。如果设备被盗，还要生成新的恢复密钥。

## 7. 与好友协作

从你账号旁的菜单中打开“好友”。发送你的邀请，或粘贴好友的邀请或用户名。有了邀请，Kodosi 就能验证你的好友。在手机上，你可以把邀请显示为二维码：好友用相机或在 Kodosi 中扫描，并在请求发出前确认信任你。

<p align="center"><img src="../media/friends.webp" width="900" alt="“好友”页面，其中有两位已验证的好友。"></p>

要共享终端，请在其标题栏的操作按钮中选择“共享”，然后选择好友或项目。与你共享的人会获得完全控制权，并在那台电脑上拥有你的权限。你的其他终端仍然是私有的。

项目汇集来自任意文件夹和电脑的终端，以及对话和任务。选择“项目”旁的 +，然后在项目中启动终端，或把一个终端共享到项目里。可随时邀请好友。

<p align="center"><img src="../media/project.webp" width="900" alt="一个项目，里面有一个共享终端，以及人和 Agent 的对话。"></p>

- 所有成员都可以使用共享终端，也可以共享自己的终端。后来加入的人也会获得这些终端和完整的对话。文件留在各个终端所在的电脑上。
- 你可以发布任务供人认领，认领、放弃认领，带着结果关闭任务，或重新打开任务。
- 关联 GitHub 或 Gitea 仓库，即可把 Issue 变成任务；当你修改这样的任务时，Kodosi 会同步修改对应的 Issue。GitHub 使用你的 `gh` 登录或 Git credential helper，Gitea 使用 credential helper，或为 `KODOSI_GITEA_URL` 中的服务器使用 `KODOSI_GITEA_TOKEN`。你的凭据留在你的电脑上。
- 在聊天中输入 `@` 和终端名称，Kodosi 会把你的消息连同你的名字和项目交给那个终端里的 Agent；如何处理由 Agent 决定。Kodosi 终端中的 Claude Code 和 Copilot CLI，以及由 Kodosi 启动的 Codex，都知道自己运行在 Kodosi 中，`kodosi skill` 会输出让 Agent 参与协作的 [skill](../../runtime/skills/kodosi/SKILL.md)。在 Kodosi 终端中，这条命令会自动找到项目；在其他地方，请加上 `--project <name>`。

## 8. 使用命令行

`kodosi` 命令与应用操作的是同一批终端，应用会显示它所做的事。

| 命令 | 作用 |
| --- | --- |
| `kodosi terminal start --agent claude --prompt "fix the failing test"` | 按你上次的选择启动 Agent。`--account`、`--model`、`--effort`、`--permissions` 和 `--output` 可以更改选择。 |
| `kodosi terminal start --directory <repository> --branch <name>` | 在新分支上、独立的 worktree 中启动终端。 |
| `kodosi terminal start --computer <computer> --directory <folder>` | 在你的另一台电脑上启动终端。相对路径的文件夹或以 `~` 开头的文件夹位于那台电脑上你的主目录中。`kodosi devices list` 会显示哪些电脑在线。 |
| `kodosi agent list` | 显示 Agent、它们的账号和你上次的选择。 |
| `kodosi agent usage` | 显示每个已登录的 Claude Code、Codex 和 Copilot CLI 账号已使用了多少套餐额度。 |
| `kodosi terminal list` | 显示你的终端及其名称和 ID。两者都可以作为 `<terminal>` 使用。 |
| `kodosi terminal read <terminal>` | 输出屏幕内容。`--history` 会加上已滚出屏幕的行。 |
| `kodosi terminal message <terminal> "Can you review my branch?"` | 把一条消息交给终端里的 Agent，并附上发送者和来源。 |
| `kodosi terminal agent <terminal> --model sonnet` | 在终端里的 Agent 运行时更改它的模型、推理强度或权限。 |
| `kodosi terminal input <terminal> --enter "make test"` | 输入一行内容。 |
| `kodosi terminal keys <terminal> ctrl-c` | 按下按键。 |
| `kodosi terminal wait <terminal> --until done,failed` | 等待某个状态。 |
| `kodosi terminal show <terminal>` | 在应用中显示该终端。 |
| `kodosi dosi say "What needs me?"` | 向 Dosi 提问。`kodosi dosi show` 输出对话，`kodosi dosi stop` 让 Dosi 停止。 |
| `kodosi show <file>` | 在当前终端的画布上显示一个文件，并为正在查看该终端的人打开画布；`kodosi show <address>` 在其浏览器中显示一个网址。`kodosi canvas` 列出正在显示的内容，`kodosi canvas remove <file>` 移除一个文件。 |
| `kodosi browser open <address>` | 在当前终端的共享浏览器中打开一个页面。其他 agent-browser 命令也都可以使用。 |
| `kodosi skill` | 输出让 Agent 与 Kodosi 中的人和 Agent 协作的 skill。`kodosi skill canvas` 输出画布的 skill。 |
| `kodosi dosi skill` | 输出让其他 Agent 充当 Dosi 的 skill。 |
| `kodosi host` | 退出应用后让你的终端继续运行。`kodosi host stop` 会停止在后台运行的主机。 |

当你在某个终端中输入时，`message`、`input` 和 `keys` 会显示一条消息并停止。Agent 正在等待某人回答时，`message` 也会停止。

## 如果连不上

请确认终端所在的电脑在线且已打开 Kodosi，并确认该终端仍处于共享状态。新设备可能需要批准，身份已变更的好友可能需要在“好友”中重新验证。连接中断后，Kodosi 会重新显示当前状态。如果另一台电脑的终端显示“离线”，说明这台设备连不上服务器，请检查它的网络。Kodosi 会自动重新连接，选择“重试”可立即连接。当 Kodosi 提示某台电脑离线时，它的终端会在它恢复后重新打开，除非这些终端已在那台电脑上结束。要把这样的终端从列表中移除，请选择它的 X。“隐藏，直到它重新上线”会在那台电脑恢复时再次显示它，“永久关闭”则会在那时将其关闭。如果 Kodosi 提示不确定你的输入是否已送达，请先查看终端，再重新输入命令。
