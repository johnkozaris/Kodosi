#!/usr/bin/env python3
"""Check that each text that people read has its words in each language of Kodosi, with the
same values as the English text."""

import json
import re
import sys
import xml.etree.ElementTree as ElementTree
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
LANGUAGES = ["de", "el", "es", "fr", "it", "ja", "ko", "zh-Hans"]
problems: list[str] = []


def report(where: Path | str, what: str) -> None:
    problems.append(f"{Path(where).relative_to(ROOT) if isinstance(where, Path) else where}: {what}")


def names(text: str) -> list[str]:
    return sorted(re.findall(r"\{(\w*)\}", text))


def compare_table(path: Path, english: set[str]) -> None:
    table = json.loads(path.read_text(encoding="utf-8"))
    for words in sorted(english - table.keys()):
        report(path, f"no words for {words!r}")
    for words in sorted(table.keys() - english):
        report(path, f"words for a text that the code does not have: {words!r}")
    for words, translated in table.items():
        if names(words) != names(translated):
            report(path, f"other values than {words!r}: {translated!r}")


def runtime_texts() -> set[str]:
    texts = set()
    for path in [*ROOT.glob("runtime/src/**/*.rs"), *ROOT.glob("runtime/crates/**/*.rs")]:
        for match in re.finditer(r'text!\(\s*"((?:[^"\\]|\\.|\\\n)*)"', path.read_text(encoding="utf-8")):
            words = re.sub(r"\\\n\s*", "", match.group(1))
            texts.add(words.replace('\\"', '"').replace("\\n", "\n").replace("\\\\", "\\"))
    return texts


def view_texts() -> set[str]:
    texts = set()
    for path in ROOT.glob("documents/views/*/src/*.ts"):
        for match in re.finditer(r'\bsay\(\s*("(?:[^"\\]|\\.)*")', path.read_text(encoding="utf-8")):
            texts.add(json.loads(match.group(1)))
    return texts


SPECIFIER = re.compile(r"%(?:\d+\$)?(lld|llu|ld|lu|d|u|@|f)")


def specifiers(text: str) -> list[str]:
    return sorted(SPECIFIER.findall(text.replace("%%", "")))


def check_catalog(path: Path) -> None:
    for key, entry in json.loads(path.read_text(encoding="utf-8"))["strings"].items():
        if entry.get("shouldTranslate") is False or entry.get("extractionState") == "stale":
            continue
        localizations = entry.get("localizations", {})
        english = localizations.get("en", {})
        source = english.get("stringUnit", {}).get("value") or (
            english.get("variations", {}).get("plural", {}).get("other", {}).get("stringUnit", {}).get("value") or key
        )
        for language in LANGUAGES:
            localization = localizations.get(language)
            if localization is None:
                report(path, f"no {language} words for {key!r}")
                continue
            units = (
                [localization["stringUnit"]]
                if "stringUnit" in localization
                else [form["stringUnit"] for form in localization.get("variations", {}).get("plural", {}).values()]
            )
            if not units:
                report(path, f"no {language} words for {key!r}")
            for unit in units:
                if unit.get("state") != "translated":
                    report(path, f"{language} words for {key!r} are not finished")
                if specifiers(unit.get("value", "")) != specifiers(source):
                    report(path, f"{language} words for {key!r} have other values: {unit.get('value')!r}")


QT_FORMS = {"de": 2, "el": 2, "en": 2, "es": 2, "fr": 2, "it": 2, "ja": 1, "ko": 1, "zh_CN": 1}


def check_qt(path: Path) -> None:
    language = path.stem.removeprefix("kodosi_")
    for message in ElementTree.parse(path).iter("message"):
        source = message.findtext("source") or ""
        translation = message.find("translation")
        if translation is None or translation.get("type") in ("unfinished", "vanished", "obsolete"):
            report(path, f"no finished words for {source!r}")
            continue
        numerus = message.get("numerus") == "yes"
        forms = [form.text or "" for form in translation.iter("numerusform")] if numerus else [translation.text or ""]
        if numerus and len(forms) != QT_FORMS[language]:
            report(path, f"{len(forms)} plural forms for {source!r}")
        for form in forms:
            if not form or sorted(re.findall(r"%\d", form)) != sorted(re.findall(r"%\d", source)):
                report(path, f"other values than {source!r}: {form!r}")
            elif numerus and "%n" not in form:
                report(path, f"no count in {form!r}")


def object_entries(text: str) -> dict[str, str]:
    entries = {}
    pattern = r"""("[^"]+"|[A-Za-z_$][\w$]*)\s*:\s*("(?:[^"\\]|\\.)*"|'(?:[^'\\]|\\.)*')"""
    for match in re.finditer(pattern, text):
        entries[match.group(1).strip('"')] = match.group(2)[1:-1]
    return entries


def object_after(text: str, start: int) -> str:
    depth = 0
    index = text.index("{", start)
    while index < len(text):
        character = text[index]
        if character in "\"'":
            index += 1
            while text[index] != character:
                index += 2 if text[index] == "\\" else 1
        elif text.startswith("//", index):
            index = text.index("\n", index)
        elif character == "{":
            depth += 1
        elif character == "}":
            depth -= 1
            if depth == 0:
                return text[start:index]
        index += 1
    raise ValueError(f"no end of the object at {start}")


def compare_words(path: Path, tables: dict[str, dict[str, str]]) -> None:
    english = tables.pop("en")
    for language, table in tables.items():
        for key in sorted(english.keys() - table.keys()):
            report(path, f"no {language} words for {key}")
        for key in sorted(table.keys() & english.keys()):
            if sorted(re.findall(r"\{\d\}", table[key])) != sorted(re.findall(r"\{\d\}", english[key])):
                report(path, f"{language} words for {key} have other values: {table[key]!r}")


def check_sign_in() -> None:
    login = ROOT / "sign-in/src/login/i18n.ts"
    code = login.read_text(encoding="utf-8")
    custom = object_after(code, code.index(".withCustomTranslations(") + 1)
    starts = {m.group(1).strip('"'): m.start() for m in re.finditer(r'^    ("?[\w-]+"?): \{', custom, re.M)}
    compare_words(login, {tag: object_entries(object_after(custom, start)) for tag, start in starts.items()})

    account = ROOT / "sign-in/src/account/words.ts"
    tables = {}
    for path in [account, *sorted((account.parent / "words").glob("*.ts"))]:
        code = path.read_text(encoding="utf-8")
        for m in re.finditer(r"^const (\w+)(?:: Table)? = \{", code, re.M):
            tables[m.group(1)] = object_entries(object_after(code, m.start()))
    compare_words(account, {tag: table for tag, table in tables.items() if tag in ("en", "zh") or tag in LANGUAGES})
    for language in ("de", "el", "es", "fr", "it", "ja", "ko", "zh"):
        if language not in tables:
            report(account.parent / "words", f"no {language}.ts")

    folder = ROOT / "sign-in/src/email/messages"
    mails = {}
    for path in folder.glob("messages_*.properties"):
        mails[path.stem.removeprefix("messages_")] = {
            key: value
            for key, _, value in (line.partition("=") for line in path.read_text(encoding="utf-8").splitlines())
            if key and not key.startswith("#")
        }
    for language, table in mails.items():
        for key, value in table.items():
            if re.search(r"(?<!')'(?!')", value):
                report(folder / f"messages_{language}.properties", f"{key} has a single apostrophe")
    compare_words(folder / "messages_en.properties", mails)
    for language in ("de", "el", "es", "fr", "it", "ja", "ko", "zh_CN"):
        if language not in mails:
            report(folder, f"no messages_{language}.properties")


def check_backend_refusals() -> None:
    http = ROOT / "runtime/src/network/http.rs"
    code = http.read_text(encoding="utf-8")
    start = code.index("fn backend_refusals()")
    refusals = code[start : code.index("\n}\n", start)]
    backend = "\n".join(path.read_text(encoding="utf-8") for path in ROOT.glob("backend/src/**/*.cs"))
    for match in re.finditer(r'text!\(\s*"((?:[^"\\]|\\.)*)"', refusals):
        if f'"{match.group(1)}"' not in backend:
            report(http, f"the backend does not refuse with {match.group(1)!r}")


def main() -> int:
    english = runtime_texts()
    for language in LANGUAGES:
        compare_table(ROOT / f"runtime/texts/{language}.json", english)
    english = view_texts()
    for language in LANGUAGES:
        compare_table(ROOT / f"documents/views/words/src/{language}.json", english)
    for path in sorted(
        [
            *ROOT.glob("clients/apple/Sources/*/Resources/*.xcstrings"),
            *ROOT.glob("clients/macos/Resources/*.xcstrings"),
            *ROOT.glob("clients/ios/Resources/*.xcstrings"),
        ]
    ):
        check_catalog(path)
    for path in sorted(ROOT.glob("clients/qt/i18n/kodosi_*.ts")):
        check_qt(path)
    check_sign_in()
    check_backend_refusals()
    for problem in problems:
        print(problem, file=sys.stderr)
    if problems:
        print(f"{len(problems)} translation problems", file=sys.stderr)
        return 1
    print("Each text has its words in each language.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
