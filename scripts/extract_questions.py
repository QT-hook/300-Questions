#!/usr/bin/env python3
"""Extract a single-choice bank without modifying the source workbook (stdlib only)."""
import argparse
import json
import posixpath
import re
import zipfile
from pathlib import Path
from xml.etree import ElementTree as ET

NS = {"m": "http://schemas.openxmlformats.org/spreadsheetml/2006/main"}
HEADERS = ["题干", "答案", "选项A", "选项B", "选项C", "选项D"]


def text(element):
    return "".join(node.text or "" for node in element.findall(".//m:t", NS))


def extract(source):
    with zipfile.ZipFile(source) as archive:
        shared = []
        if "xl/sharedStrings.xml" in archive.namelist():
            shared = [text(item) for item in ET.fromstring(archive.read("xl/sharedStrings.xml"))]
        workbook = ET.fromstring(archive.read("xl/workbook.xml"))
        first = workbook.find("m:sheets/m:sheet", NS)
        rel_id = first.get("{http://schemas.openxmlformats.org/officeDocument/2006/relationships}id")
        relationships = ET.fromstring(archive.read("xl/_rels/workbook.xml.rels"))
        target = next(rel.get("Target") for rel in relationships if rel.get("Id") == rel_id)
        sheet_path = target.lstrip("/") if target.startswith("/") else posixpath.normpath("xl/" + target)
        sheet = ET.fromstring(archive.read(sheet_path))
        columns = None
        result = []
        for row in sheet.findall("m:sheetData/m:row", NS):
            values = {}
            for cell in row.findall("m:c", NS):
                column = re.fullmatch(r"([A-Z]+)[0-9]+", cell.attrib["r"]).group(1)
                kind = cell.get("t")
                raw = cell.findtext("m:v", "", NS)
                values[column] = shared[int(raw)] if kind == "s" else text(cell) if kind == "inlineStr" else raw
            if not any(value.strip() for value in values.values()):
                continue
            if columns is None:
                if all(header in values.values() for header in HEADERS):
                    columns = {header: next(key for key, value in values.items() if value == header) for header in HEADERS}
                continue
            fields = {header: values.get(column, "") for header, column in columns.items()}
            if any(not value.strip() for value in fields.values()) or fields["答案"] not in "ABCD" or len(fields["答案"]) != 1:
                raise ValueError(f"Incomplete question or invalid answer at worksheet row {row.get('r')}")
            result.append({"question": fields["题干"], "options": [fields[f"选项{letter}"] for letter in "ABCD"], "answer": fields["答案"]})
        if not result:
            raise ValueError("No questions/header found")
        return result


def compare_markdown(questions, path):
    rows = []
    for line in path.read_text(encoding="utf-8").splitlines()[2:]:
        if not line.startswith("|"):
            continue
        cells = [part.strip().replace("<br>", "\n").replace("\\|", "|") for part in re.split(r"(?<!\\)\|", line)[1:-1]]
        if len(cells) != 10:
            raise ValueError("Unexpected audit table column count")
        rows.append({"question": cells[4], "options": cells[6:10], "answer": cells[5]})
    if questions != rows:
        mismatch = next((i + 1 for i, pair in enumerate(zip(questions, rows)) if pair[0] != pair[1]), "length")
        raise ValueError(f"Audit markdown differs at question {mismatch}")
    print(f"Audit: all {len(rows)} questions, options and answers match in order")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("source", type=Path)
    parser.add_argument("output", type=Path)
    parser.add_argument("--audit-markdown", type=Path)
    parser.add_argument("--force", action="store_true", help="Replace an existing JSON output")
    args = parser.parse_args()
    if args.source.resolve() == args.output.resolve():
        parser.error("Output must not overwrite the source")
    if args.output.suffix.lower() != ".json":
        parser.error("Output must be a .json file")
    if args.output.exists() and not args.force:
        parser.error("Output exists; use --force to explicitly replace it")
    questions = extract(args.source)
    if args.audit_markdown:
        compare_markdown(questions, args.audit_markdown)
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(questions, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
    print(f"Extracted {len(questions)} complete questions to {args.output}")


if __name__ == "__main__":
    main()
