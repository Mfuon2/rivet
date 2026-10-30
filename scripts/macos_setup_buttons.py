"""Exercise native accessibility button actions and the OS clipboard in an isolated macOS Rivet process."""
import argparse
import json
import os
from pathlib import Path
import platform
import subprocess
import tempfile
import time

ROOT = Path(__file__).resolve().parents[1]
SWIFT = r'''
import AppKit
import CoreGraphics
import ApplicationServices
import Vision
import Foundation
let args = CommandLine.arguments
switch args[1] {
case "backup":
    let items = NSPasteboard.general.pasteboardItems?.map { item in
        Dictionary(uniqueKeysWithValues: item.types.compactMap { type in item.data(forType: type).map { (type.rawValue, $0) } })
    } ?? []
    let data = try PropertyListSerialization.data(fromPropertyList: items, format: .binary, options: 0)
    try data.write(to: URL(fileURLWithPath: args[2]), options: .atomic)
case "restore":
    let data = try Data(contentsOf: URL(fileURLWithPath: args[2]))
    let values = try PropertyListSerialization.propertyList(from: data, options: [], format: nil) as! [[String: Data]]
    let items = values.map { values in
        let item = NSPasteboardItem()
        for (type, data) in values { item.setData(data, forType: NSPasteboard.PasteboardType(type)) }
        return item
    }
    NSPasteboard.general.clearContents()
    NSPasteboard.general.writeObjects(items)
case "position":
    let pid = Int32(args[2])!
    NSRunningApplication(processIdentifier: pid)?.activate(options: [.activateIgnoringOtherApps])
    let app = AXUIElementCreateApplication(pid)
    var value: CFTypeRef?
    if AXUIElementCopyAttributeValue(app, kAXWindowsAttribute as CFString, &value) == .success,
       let windows = value as? [AXUIElement], let window = windows.first {
        var point = CGPoint(x: 80, y: 80)
        AXUIElementSetAttributeValue(window, kAXPositionAttribute as CFString, AXValueCreate(.cgPoint, &point)!)
    }
case "window":
    let pid = Int32(args[2])!
    let windows = CGWindowListCopyWindowInfo([.optionOnScreenOnly, .excludeDesktopElements], kCGNullWindowID) as! [[String: Any]]
    let owned = windows.filter { ($0[kCGWindowOwnerPID as String] as? Int32) == pid && ($0[kCGWindowLayer as String] as? Int) == 0 }
    guard let window = owned.first else { exit(2) }
    let bounds = window[kCGWindowBounds as String] as! [String: Any]
    let data = try JSONSerialization.data(withJSONObject: ["id":window[kCGWindowNumber as String]!, "bounds":bounds])
    print(String(data: data, encoding: .utf8)!)
case "ocr":
    let image = NSImage(contentsOfFile: args[2])!
    var rect = CGRect(origin: .zero, size: image.size)
    let cg = image.cgImage(forProposedRect: &rect, context: nil, hints: nil)!
    let request = VNRecognizeTextRequest()
    request.recognitionLevel = .accurate
    try VNImageRequestHandler(cgImage: cg).perform([request])
    let rows = (request.results ?? []).compactMap { result -> [String: Any]? in
        guard let text = result.topCandidates(1).first?.string else { return nil }
        return ["text":text, "x":result.boundingBox.midX, "y":1-result.boundingBox.midY]
    }
    print(String(data: try JSONSerialization.data(withJSONObject: rows), encoding: .utf8)!)
case "labels":
    let app = AXUIElementCreateApplication(Int32(args[2])!)
    var labels = Set<String>()
    func collect(_ node: AXUIElement, _ depth: Int = 0) {
        if depth > 40 { return }
        for key in [kAXTitleAttribute, kAXDescriptionAttribute, kAXValueAttribute] {
            var value: CFTypeRef?
            if AXUIElementCopyAttributeValue(node, key as CFString, &value) == .success, let text = value as? String { labels.insert(text) }
        }
        var value: CFTypeRef?
        if AXUIElementCopyAttributeValue(node, kAXChildrenAttribute as CFString, &value) == .success, let children = value as? [AXUIElement] {
            for child in children { collect(child, depth + 1) }
        }
    }
    var value: CFTypeRef?
    if AXUIElementCopyAttributeValue(app, kAXWindowsAttribute as CFString, &value) == .success, let windows = value as? [AXUIElement] {
        for window in windows { collect(window) }
    }
    print(String(data: try JSONSerialization.data(withJSONObject: Array(labels).map { ["text": $0] }), encoding: .utf8)!)
case "press":
    let pid = Int32(args[2])!
    NSRunningApplication(processIdentifier: pid)?.activate(options: [.activateIgnoringOtherApps])
    Thread.sleep(forTimeInterval: 0.3)
    let app = AXUIElementCreateApplication(pid)
    func find(_ node: AXUIElement, _ label: String, _ depth: Int = 0) -> AXUIElement? {
        if depth > 40 { return nil }
        for key in [kAXTitleAttribute, kAXDescriptionAttribute, kAXValueAttribute] {
            var value: CFTypeRef?
            if AXUIElementCopyAttributeValue(node, key as CFString, &value) == .success, let text = value as? String, text == label {
                var role: CFTypeRef?
                AXUIElementCopyAttributeValue(node, kAXRoleAttribute as CFString, &role)
                if let role = role as? String, [kAXButtonRole, kAXCheckBoxRole, kAXRadioButtonRole, kAXDisclosureTriangleRole].contains(role) { return node }
            }
        }
        var value: CFTypeRef?
        if AXUIElementCopyAttributeValue(node, kAXChildrenAttribute as CFString, &value) == .success, let children = value as? [AXUIElement] {
            for child in children { if let match = find(child, label, depth + 1) { return match } }
        }
        return nil
    }
    for _ in 0..<30 {
        var value: CFTypeRef?
        if AXUIElementCopyAttributeValue(app, kAXWindowsAttribute as CFString, &value) == .success, let windows = value as? [AXUIElement] {
            for window in windows {
                if let button = find(window, args[3]) {
                    guard AXUIElementPerformAction(button, kAXPressAction as CFString) == .success else { exit(3) }
                    exit(0)
                }
            }
        }
        Thread.sleep(forTimeInterval: 0.2)
    }
    fputs("Button not exposed to accessibility: \(args[3])\n", stderr)
    exit(4)
default: exit(2)
}
'''

def main():
    if platform.system() != "Darwin":
        raise SystemExit("This native interaction check runs on macOS only")
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--screenshots", action="store_true", help="Seed a local demo queue and refresh owned-window documentation previews")
    args = parser.parse_args()
    with tempfile.TemporaryDirectory(prefix="rivet-buttons-") as temporary:
        folder = Path(temporary)
        swift = folder / "buttons.swift"
        swift.write_text(SWIFT)
        helper = folder / "buttons"
        subprocess.run(["swiftc", str(swift), "-o", str(helper)], check=True, capture_output=True)
        def native(*args):
            return subprocess.run([str(helper), *map(str, args)], check=True, capture_output=True, text=True, timeout=30).stdout
        backup = folder / "clipboard.plist"
        native("backup", backup)
        env = dict(os.environ, RIVET_DATABASE=str(folder / "queue.sqlite3"), RIVET_SETUP_ROOT=str(folder / "home"))
        if args.screenshots:
            examples = ["project-source.zip", "release-notes.pdf", "conference-talk.mp4", "ambient-session.flac", "rivet-setup.exe", "research-data.csv"]
            for index in range(24):
                name = f"{index+1:02d}-{examples[index % len(examples)]}"
                subprocess.run([str(ROOT / "target/debug/rivet"), "add", "https://example.test/" + name, "-o", str(folder / name)], env=env, check=True, capture_output=True)
        process = subprocess.Popen([str(ROOT / "target/debug/rivet-desktop")], env=env, cwd=ROOT, stdout=subprocess.DEVNULL, stderr=subprocess.PIPE)
        try:
            time.sleep(2)
            native("position", process.pid)
            time.sleep(0.5)
            window = json.loads(native("window", process.pid))
            if args.screenshots:
                subprocess.run(["screencapture", "-x", "-l", str(window["id"]), str(ROOT / "docs/startup-preview.png")], check=True, capture_output=True)
            def text():
                return json.loads(native("labels", process.pid))
            def click(label):
                native("press", process.pid, label)
                time.sleep(0.4)
            subprocess.run(["pbcopy"], input="rivet-native-test-sentinel", text=True, check=True)
            click("Connection details")
            click("Development setup")
            click("Copy browser setup address")
            address = subprocess.check_output(["pbpaste"], text=True)
            assert address in ("chrome://extensions", "edge://extensions", "about:debugging#/runtime/this-firefox"), "System clipboard did not receive the browser address"
            click("Copy extension folder")
            path = subprocess.check_output(["pbpaste"], text=True)
            assert Path(path).is_dir() and Path(path).resolve().is_relative_to(folder.resolve()), f"System clipboard did not receive the prepared extension path (still address: {path == address})"
            click("Check again")
            time.sleep(1)
            if not any(row["text"] == "Copy browser setup address" for row in text()):
                if not any(row["text"] == "Development setup" for row in text()):
                    click("Connection details")
                click("Development setup")
            assert any(row["text"] == "Copy browser setup address" for row in text()), "Setup did not return after rechecking"
            click("Continue to Rivet")
            assert any("Categories" in row["text"] for row in text()), "Continue did not open the download manager"
            if args.screenshots:
                subprocess.run(["screencapture", "-x", "-l", str(window["id"]), str(ROOT / "docs/desktop-preview.png")], check=True, capture_output=True)
            print(json.dumps({"native_copy_address": "passed", "native_copy_folder": "passed", "check_again": "passed", "continue_to_manager": "passed", "registration": "isolated temporary home", "clipboard": "restored after test"}, indent=2))
        finally:
            if process.poll() is None:
                process.terminate()
            try: process.communicate(timeout=10)
            except subprocess.TimeoutExpired:
                process.kill(); process.communicate()
            native("restore", backup)

if __name__ == "__main__": main()
