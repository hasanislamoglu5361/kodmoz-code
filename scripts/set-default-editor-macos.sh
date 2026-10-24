#!/usr/bin/env bash
set -euo pipefail

if [ "$(uname -s)" != "Darwin" ]; then
  echo "Bu script yalnizca macOS icin." >&2
  exit 1
fi

APP="/Applications/Kodmoz Code.app"
if [ ! -d "$APP" ]; then
  APP="$HOME/Applications/Kodmoz Code.app"
fi
if [ ! -d "$APP" ]; then
  echo "Kodmoz Code bulunamadi. Once ./scripts/install-macos.sh calistirin." >&2
  exit 1
fi

swift - "$APP" <<'SWIFT'
import AppKit
import Foundation
import CoreServices
import UniformTypeIdentifiers

let app = URL(fileURLWithPath: CommandLine.arguments[1])
let bundleID = Bundle(url: app)?.bundleIdentifier ?? "com.kodmoz.code"
let bundleIdentifier = bundleID as NSString

func setDefault(_ type: UTType) {
    let identifier = type.identifier
    let status = LSSetDefaultRoleHandlerForContentType(
        identifier as NSString,
        LSRolesMask.all,
        bundleIdentifier
    )
    var current = "nil"
    if let handler = LSCopyDefaultRoleHandlerForContentType(identifier as NSString, LSRolesMask.all) {
        current = handler.takeRetainedValue() as String
    }
    print("\(identifier): status=\(status) handler=\(current)")
}

let identifiers = [
    "public.text",
    "public.plain-text",
    "public.source-code",
    "public.script",
    "public.json",
    "public.xml",
    "public.yaml",
    "public.html",
    "public.css",
    "com.netscape.javascript-source",
    "public.php-script",
    "public.ruby-script",
    "public.perl-script",
    "public.objective-c-source",
    "com.sun.java-source",
    "com.kodmoz.terraform",
    "public.python-script",
    "public.shell-script",
    "public.c-source",
    "public.c-plus-plus-source",
    "public.swift-source",
]

for identifier in identifiers {
    guard let type = UTType(identifier) else {
        print("atlandi: \(identifier)")
        continue
    }
    setDefault(type)
}

// Concrete and dynamic UTIs for common text/source extensions.
let extensions = [
    "txt", "log", "md", "markdown",
    "rs", "php", "swift", "sh", "bash", "zsh", "fish",
    "yaml", "yml", "toml", "json", "jsonc",
    "js", "mjs", "cjs", "jsx", "tsx",
    "py", "go", "c", "h", "cpp", "cc", "hpp",
    "java", "kt", "rb", "lua", "sql",
    "html", "htm", "css", "scss", "xml", "svg",
    "tf", "tfvars", "hcl", "ini", "cfg", "conf",
]
var seen = Set<String>()
for ext in extensions {
    guard let type = UTType(filenameExtension: ext) else { continue }
    if type.identifier == "public.data" { continue }
    if seen.insert(type.identifier).inserted {
        setDefault(type)
    }
}

print("")
print("Kodmoz Code varsayilan metin/kaynak kodu editoru olarak ayarlandi.")
SWIFT
