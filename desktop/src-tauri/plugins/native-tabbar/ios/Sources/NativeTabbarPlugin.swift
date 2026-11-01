import SwiftRs
import Tauri
import UIKit
import WebKit
import os

private let log = Logger(subsystem: "app.laab.ios", category: "native-tabbar")

struct TabArgs: Decodable {
  let id: String
  let title: String
  /// SF Symbol names for the normal and selected states.
  let icon: String
  let selectedIcon: String?
  let badge: String?
}

/// A bar button for the vertical bar. Tabs are sent as buttons with
/// `selected` set too, and skipped: the tab bar shows them.
struct SideButtonArgs: Codable {
  let id: String
  let icon: String
  let selectedIcon: String?
  let selected: Bool?
  let badge: String?
}

/// The bar button groups for the vertical bar, the top ones first.
struct SideArgs: Codable {
  let top: [[SideButtonArgs]]
  let bottom: [[SideButtonArgs]]
}

struct UpdateArgs: Decodable {
  let tabs: [TabArgs]
  let selected: String
  /// Hides both bars (a dialog is over the page, or nobody is signed in).
  let hidden: Bool
  /// Hides only the bottom tab bar (a chat is pushed over the list); the
  /// vertical bar stays and shows the chat's buttons.
  let bottomHidden: Bool?
  let side: SideArgs?
  /// The page's menu, for the system overflow menu in the vertical bar.
  let menu: [[MenuItemArgs]]?
}

/// What a tab's bar items are built from, to tell when they change.
struct BarItems: Encodable {
  let top: [[SideButtonArgs]]
  let bottom: [[SideButtonArgs]]
  let menu: [[MenuItemArgs]]
}

/// A menu item; choosing one sends the `action` `menu:<id>`.
struct MenuItemArgs: Codable {
  let id: String
  let title: String
  /// SF Symbol name.
  let icon: String?
  let destructive: Bool?
  let disabled: Bool?
  /// A checkmark item when set.
  let checked: Bool?
  let children: [[MenuItemArgs]]?
}

extension MenuItemArgs {
  /// A UIKit menu, its sections inline; choosing an item calls `run` with its id.
  static func elements(_ sections: [[MenuItemArgs]], run: @escaping (String) -> Void) -> [UIMenuElement] {
    sections.map { UIMenu(title: "", options: .displayInline, children: $0.map { $0.element(run) }) }
  }

  func element(_ run: @escaping (String) -> Void) -> UIMenuElement {
    let image = icon.flatMap { UIImage(systemName: $0) }
    if let children {
      return UIMenu(title: title, image: image, children: Self.elements(children, run: run))
    }
    var attributes: UIMenuElement.Attributes = []
    if destructive == true { attributes.insert(.destructive) }
    if disabled == true { attributes.insert(.disabled) }
    let id = id
    return UIAction(title: title, image: image, attributes: attributes, state: checked == true ? .on : .off) { _ in run(id) }
  }
}

struct RectArgs: Decodable {
  let x: Double
  let y: Double
  let width: Double
  let height: Double
}

/// The page's answer to `chatMenu`: the chat's menu, and where it drew the
/// chat's preview card for the system to show over the menu (left out on
/// the iPhone Duo's cover screen, which shows the menu alone).
struct MenuArgs: Decodable {
  let token: Int
  let menu: [[MenuItemArgs]]
  let card: RectArgs?
}

/// LAAB's bars, drawn by iOS (see `SystemBars`): a tab bar, and the chat's
/// buttons and menu as bar items, which iOS puts in the vertical bar where
/// there is one (iOS 27.1, the iPhone Duo). The frontend hears `select` for
/// tabs, `action` for bar buttons and menu items, `layout` when the vertical
/// bar or the fold changes and `hinge` as a foldable opens or closes.
class NativeTabbarPlugin: Plugin {
  private var bars: SystemBars?
  private var watcher: LayoutWatcher?
  private var last: UpdateArgs?
  private var reported: String?
  private var menus: ChatMenus?

  /// Long-pressing a chat row (a link to `#chat=<id>`) shows the system's
  /// context menu: the page hears `chatMenu` and answers with `menu`, then
  /// hears `menuAction` for a chosen item and `menuCommit` for a tap on the
  /// preview.
  override func load(webview: WKWebView) {
    let menus = ChatMenus(webview: webview)
    menus.onRequest = { [weak self] token, chat in self?.trigger("chatMenu", data: ["token": token, "chatId": chat]) }
    menus.onAction = { [weak self] id in self?.trigger("menuAction", data: ["id": id]) }
    menus.onCommit = { [weak self] chat in self?.trigger("menuCommit", data: ["chatId": chat]) }
    self.menus = menus
  }

  /// The page hid the chat's card again (see `ChatMenus.freeze`).
  @objc public func thaw(_ invoke: Invoke) {
    DispatchQueue.main.async {
      self.menus?.thaw()
      invoke.resolve()
    }
  }

  @objc public func menu(_ invoke: Invoke) throws {
    let args = try invoke.parseArgs(MenuArgs.self)
    DispatchQueue.main.async {
      self.menus?.answer(args) { invoke.resolve() }
    }
  }

  @objc public func update(_ invoke: Invoke) throws {
    let args = try invoke.parseArgs(UpdateArgs.self)
    log.debug("update selected=\(args.selected, privacy: .public) hidden=\(args.hidden)")
    DispatchQueue.main.async {
      guard let root = self.manager.viewController?.view else {
        invoke.reject("no view to show the bars in")
        return
      }
      self.last = args
      self.apply(args, in: root)
      invoke.resolve(self.layoutInfo())
    }
  }

  private func apply(_ args: UpdateArgs, in root: UIView) {
    let watcher = self.watcher ?? installWatcher(in: root)
    if bars == nil, let content = manager.viewController, let window = content.view.window {
      let bars = SystemBars(content: content, window: window)
      bars.onSelect = { [weak self] id in self?.trigger("select", data: ["id": id]) }
      bars.onAction = { [weak self] id in self?.trigger("action", data: ["id": id]) }
      self.bars = bars
    }
    bars?.update(args, vertical: watcher.edge != nil)
  }

  /// `edge` is "leading", "trailing" or nil; `fold` is the frame of the
  /// hinge while the device is partly open; `topClear` is where the page may
  /// start at the top outside the camera's corner (see `LayoutWatcher`). The tab
  /// bar's room is in the page's bottom safe area.
  private func layoutInfo() -> JSObject {
    var info: JSObject = [:]
    guard let watcher else { return info }
    let rect = { (r: CGRect) -> JSObject in
      ["x": Double(r.minX), "y": Double(r.minY), "width": Double(r.width), "height": Double(r.height)]
    }
    if let edge = watcher.edge { info["edge"] = edge }
    if let fold = watcher.fold { info["fold"] = rect(fold) }
    info["topClear"] = Double(watcher.topClear)
    return info
  }

  private func installWatcher(in root: UIView) -> LayoutWatcher {
    let column = LayoutWatcher(frame: root.bounds)
    column.autoresizingMask = [.flexibleWidth, .flexibleHeight]
    // Folding, unfolding and rotating move or remove the vertical bar, the
    // fold and the camera's corner.
    column.onChange = { [weak self] in
      guard let self, let root = self.manager.viewController?.view, let args = self.last else { return }
      self.apply(args, in: root)
      let key = self.watcher?.key
      if key != self.reported {
        self.reported = key
        self.trigger("layout", data: self.layoutInfo())
      }
    }
    if #available(iOS 27.1, *) {
      // The hinge's status and angle in degrees, which reveal a chat peeked
      // at from the cover screen. Devices without a hinge never call this.
      column.addInteraction(
        UIHingeInteraction { [weak self] _, update in
          guard let hinge = update.hinge else { return }
          let status: String =
            switch hinge.status {
            case .closed: "closed"
            case .partiallyOpen: "partial"
            case .fullyOpen: "open"
            default: "unknown"
            }
          let angle = Double(hinge.angle * 180 / .pi)
          log.debug("hinge \(status, privacy: .public) \(angle, privacy: .public)")
          self?.trigger("hinge", data: ["status": status, "angle": angle])
        })
    }
    root.addSubview(column)
    self.watcher = column
    reported = column.key
    return column
  }
}

/// An invisible view over the page that reports what UIKit gives through a
/// view's traits and reserved regions: the vertical bar's edge, the fold, and
/// where the top of the screen is clear.
class LayoutWatcher: UIView {
  var onChange: (() -> Void)?
  private var known: String?

  override init(frame: CGRect) {
    super.init(frame: frame)
    backgroundColor = .clear
    isUserInteractionEnabled = false
    if #available(iOS 27.1, *) {
      registerForTraitChanges(UITraitCollection.systemTraitsAffectingVerticalBarEdge) {
        (view: LayoutWatcher, _: UITraitCollection) in
        view.setNeedsLayout()
      }
    }
    known = key
  }

  required init?(coder: NSCoder) { fatalError("init(coder:) is not used") }

  /// The vertical bar's edge, or nil when this screen has none.
  var edge: String? {
    if #available(iOS 27.1, *) {
      switch traitCollection.verticalBarEdge {
      case .leading: return "leading"
      case .trailing: return "trailing"
      default: return nil
      }
    }
    return nil
  }

  /// The active fold (a division region) in this view's coordinates, which
  /// are the webview's: present only while the device is partly open.
  var fold: CGRect? {
    if #available(iOS 27.1, *) {
      return reservedRegions(kind: .division).first?.frame
    }
    return nil
  }

  /// The top inset outside the occlusions. Where a status bar spans the top,
  /// that is the whole safe area. The iPhone Duo's inner screen keeps its
  /// status bar and camera in one corner, so its 82 pt top inset is needed
  /// only there: elsewhere the page may start near the top, as the App
  /// Store's title does, while what sits under the corner moves aside.
  var topClear: CGFloat {
    let bar = window?.windowScene?.statusBarManager?.statusBarFrame.height ?? 0
    return bar >= 10 || topOcclusions.isEmpty ? safeAreaInsets.top : 0
  }

  /// Occlusions (the camera and the status bar beside it) at the top edge.
  var topOcclusions: [CGRect] {
    if #available(iOS 27.1, *) {
      return reservedRegions(kind: .occlusion).map(\.frame).filter { $0.minY <= 1 && $0.width > 0 }
    }
    return []
  }

  /// Everything this view reports, to tell when it changed.
  var key: String {
    "\(edge ?? "")|\(fold.map { NSCoder.string(for: $0) } ?? "")|\(topClear)|"
      + topOcclusions.map { NSCoder.string(for: $0) }.joined(separator: ",")
  }

  override func safeAreaInsetsDidChange() {
    super.safeAreaInsetsDidChange()
    setNeedsLayout()
  }

  // UIKit runs this again when the regions read here change, so folding and
  // rotating the device land here too.
  override func layoutSubviews() {
    super.layoutSubviews()
    let now = key
    if now != known {
      known = now
      DispatchQueue.main.async { self.onChange?() }
    }
  }
}

/// LAAB's bars as the system's own. Each tab is a navigation controller, and
/// Tauri's view controller moves into the selected one, so iOS draws the tab
/// bar and the chat's bar buttons itself: at the bottom and top, or in the
/// vertical bar where there is one (the iPhone Duo).
final class SystemBars: NSObject, UITabBarControllerDelegate {
  var onSelect: ((String) -> Void)?
  var onAction: ((String) -> Void)?
  private let tabs = UITabBarController()
  private let content: UIViewController
  private var ids: [String] = []
  /// The tabs' symbols as last set, and what each tab's bar items and the
  /// tab bar's visibility were last built from. The page sends an update on
  /// every change; replacing items that did not change makes iOS rebuild
  /// them, which cuts short the tab bar's own animations (a press sliding
  /// between tabs snaps instead of settling).
  private var icons: [String] = []
  private var built: [ObjectIdentifier: Data] = [:]
  private var tabsHidden: Bool?

  init(content: UIViewController, window: UIWindow) {
    self.content = content
    super.init()
    tabs.delegate = self
    // LINE marks the selected tab black (white in dark mode), not blue.
    tabs.view.tintColor = .label
    window.rootViewController = tabs
  }

  func update(_ args: UpdateArgs, vertical: Bool) {
    setTabs(args.tabs)
    if let i = ids.firstIndex(of: args.selected), i != tabs.selectedIndex || content.parent == nil {
      tabs.selectedIndex = i
      embed()
    }
    guard let nav = tabs.selectedViewController as? UINavigationController, let host = nav.topViewController else {
      return
    }
    // The page draws its own headers; the navigation bar is only for the
    // buttons the vertical bar shows.
    let buttons = vertical && !args.hidden
    let tabIds = Set(ids.map { "tab:\($0)" })
    let top = buttons ? (args.side?.top ?? []).filter { !$0.isEmpty } : []
    let bottom = buttons ? (args.side?.bottom ?? []).filter { g in !g.isEmpty && !g.contains { tabIds.contains($0.id) } } : []
    // HIG: the page's own menu goes in the system overflow menu.
    let menu = buttons ? (args.menu ?? []).filter { !$0.isEmpty } : []
    let key = try? JSONEncoder().encode(BarItems(top: top, bottom: bottom, menu: menu))
    if let key, built[ObjectIdentifier(host)] != key || nav.isNavigationBarHidden == buttons {
      built[ObjectIdentifier(host)] = key
      nav.setNavigationBarHidden(!buttons, animated: false)
      host.navigationItem.leadingItemGroups = top.map(group)
      host.navigationItem.trailingItemGroups = bottom.map(group)
      host.navigationItem.additionalOverflowItems =
        menu.isEmpty
        ? nil
        : UIDeferredMenuElement.uncached { [weak self] done in done(self?.menuElements(menu) ?? []) }
    }
    let hideTabs = args.hidden || (args.bottomHidden ?? false)
    if hideTabs != tabsHidden {
      tabsHidden = hideTabs
      if #available(iOS 18.0, *) {
        tabs.setTabBarHidden(hideTabs, animated: true)
      } else {
        tabs.tabBar.isHidden = hideTabs
      }
    }
  }

  private func setTabs(_ next: [TabArgs]) {
    if next.map(\.id) != ids {
      ids = next.map(\.id)
      tabs.setViewControllers(
        next.map { _ in
          let host = UIViewController()
          host.view.backgroundColor = .systemBackground
          return UINavigationController(rootViewController: host)
        }, animated: false)
    }
    // Change the items in place, and only what changed.
    let names = next.map { "\($0.icon)|\($0.selectedIcon ?? "")" }
    for (i, (vc, t)) in zip(tabs.viewControllers ?? [], next).enumerated() {
      let item = vc.tabBarItem!
      if icons.count != names.count || icons[i] != names[i] {
        item.image = UIImage(systemName: t.icon)
        item.selectedImage = UIImage(systemName: t.selectedIcon ?? t.icon)
      }
      if item.title != t.title { item.title = t.title }
      if item.badgeValue != t.badge { item.badgeValue = t.badge }
    }
    icons = names
  }

  /// Tauri's page into the selected tab, filling it, under its bars.
  private func embed() {
    guard let host = (tabs.selectedViewController as? UINavigationController)?.viewControllers.first,
      content.parent !== host
    else { return }
    content.willMove(toParent: nil)
    content.view.removeFromSuperview()
    content.removeFromParent()
    host.addChild(content)
    content.view.frame = host.view.bounds
    content.view.autoresizingMask = [.flexibleWidth, .flexibleHeight]
    host.view.addSubview(content.view)
    content.didMove(toParent: host)
  }

  func tabBarController(_ tabBarController: UITabBarController, didSelect viewController: UIViewController) {
    let i = tabBarController.selectedIndex
    guard ids.indices.contains(i) else { return }
    embed()
    onSelect?(ids[i])
  }

  private func menuElements(_ sections: [[MenuItemArgs]]) -> [UIMenuElement] {
    MenuItemArgs.elements(sections) { [weak self] id in self?.onAction?("menu:\(id)") }
  }

  private static let titles = [
    "back": "กลับ", "search": "ค้นหาในแชท", "notes": "โน้ต", "compose": "ค้นหา",
  ]

  private func group(_ buttons: [SideButtonArgs]) -> UIBarButtonItemGroup {
    UIBarButtonItemGroup(
      barButtonItems: buttons.map { b in
        let item = UIBarButtonItem(
          title: Self.titles[b.id] ?? b.id, image: UIImage(systemName: b.icon),
          primaryAction: UIAction { [weak self] _ in self?.onAction?(b.id) })
        if #available(iOS 26.0, *), let badge = b.badge { item.badge = .string(badge) }
        return item
      }, representativeItem: nil)
  }
}

/// Stands in front of wry's UI delegate, which it forwards everything else
/// to, to give chat rows a system context menu. WebKit asks for a link's
/// menu and waits for the answer, so the page can build it: iOS has no API
/// to open a context menu from code.
final class ChatMenus: NSObject, WKUIDelegate {
  var onRequest: ((Int, String) -> Void)?
  var onAction: ((String) -> Void)?
  var onCommit: ((String) -> Void)?
  private weak var webview: WKWebView?
  private let original: WKUIDelegate?
  private var token = 0
  private var pending: (token: Int, done: (UIContextMenuConfiguration?) -> Void)?
  private var frozen: UIView?
  /// The item chosen from the menu, run once the menu has closed.
  private var chosen: String?

  init(webview: WKWebView) {
    self.webview = webview
    original = webview.uiDelegate
    super.init()
    webview.allowsLinkPreview = true
    webview.uiDelegate = self
  }

  override func responds(to aSelector: Selector!) -> Bool {
    super.responds(to: aSelector) || (original?.responds(to: aSelector) ?? false)
  }

  override func forwardingTarget(for aSelector: Selector!) -> Any? {
    original?.responds(to: aSelector) == true ? original : super.forwardingTarget(for: aSelector)
  }

  private static func chat(_ url: URL?) -> String? {
    guard let fragment = url?.fragment, fragment.hasPrefix("chat=") else { return nil }
    return String(fragment.dropFirst(5))
  }

  func webView(
    _ webView: WKWebView, contextMenuConfigurationForElement elementInfo: WKContextMenuElementInfo,
    completionHandler: @escaping (UIContextMenuConfiguration?) -> Void
  ) {
    guard let chat = Self.chat(elementInfo.linkURL) else {
      // Only chat rows allow the callout (see index.css).
      completionHandler(nil)
      return
    }
    pending?.done(nil)
    token += 1
    let token = token
    pending = (token, completionHandler)
    showLiftedRow()
    freeze()
    onRequest?(token, chat)
    // A page that never answers must not hold the press forever.
    DispatchQueue.main.asyncAfter(deadline: .now() + 1.5) { [weak self] in
      guard let self, self.pending?.token == token else { return }
      self.pending = nil
      self.thaw()
      completionHandler(nil)
    }
  }

  func answer(_ args: MenuArgs, then: @escaping () -> Void) {
    guard let pending, pending.token == args.token else {
      then()
      return
    }
    self.pending = nil
    // The lifted row flies back over the page while the menu closes; an
    // item that opens something (ดูโดยไม่อ่าน's card or sheet) waits for it.
    let menu = UIMenu(children: MenuItemArgs.elements(args.menu) { [weak self] id in self?.chosen = id })
    let show = { (card: UIImage?) in
      pending.done(
        UIContextMenuConfiguration(
          identifier: nil,
          previewProvider: card.map { image in { CardPreview(image: image) } },
          actionProvider: { _ in menu }))
      then()
    }
    guard let card = args.card, let webview else {
      show(nil)
      return
    }
    let config = WKSnapshotConfiguration()
    config.rect = CGRect(x: card.x, y: card.y, width: card.width, height: card.height)
    config.afterScreenUpdates = true
    webview.takeSnapshot(with: config) { image, error in
      if let error { log.error("card snapshot: \(error.localizedDescription, privacy: .public)") }
      show(image)
    }
  }

  /// A picture of the page as it is, over the web view while the page draws
  /// the chat's card to be snapshotted, so the card never shows on screen.
  /// The snapshot is of the web view's own layers, which the picture is not.
  private func freeze() {
    guard frozen == nil, let webview, let superview = webview.superview,
      let picture = webview.snapshotView(afterScreenUpdates: false)
    else { return }
    picture.frame = webview.frame
    picture.isUserInteractionEnabled = false
    superview.insertSubview(picture, aboveSubview: webview)
    frozen = picture
  }

  func thaw() {
    frozen?.removeFromSuperview()
    frozen = nil
  }

  /// WebKit calls this as the menu starts to close. The row lifted for the
  /// menu flies back into place, and WebKit keeps it there, over the real
  /// row, for almost a second more; anything the chosen item opens
  /// (ดูโดยไม่อ่าน's card or sheet) would sit under it. So the item waits for
  /// the closing animation (about 0.33 s in the simulator), and the leftover
  /// row is hidden until the next long press, when WebKit lifts a row again.
  func webView(_ webView: WKWebView, contextMenuDidEndForElement elementInfo: WKContextMenuElementInfo) {
    thaw()
    guard let id = chosen else { return }
    chosen = nil
    DispatchQueue.main.asyncAfter(deadline: .now() + 0.45) { [weak self] in
      guard let self else { return }
      self.hideLiftedRow()
      self.onAction?(id)
    }
  }

  private var hidden: [UIView] = []

  private func hideLiftedRow() {
    guard let webview else { return }
    func find(_ view: UIView) {
      if NSStringFromClass(type(of: view)).contains("TargetedPreviewContainer") {
        view.isHidden = true
        hidden.append(view)
        return
      }
      view.subviews.forEach(find)
    }
    find(webview)
  }

  private func showLiftedRow() {
    hidden.forEach { $0.isHidden = false }
    hidden = []
  }

  func webView(
    _ webView: WKWebView, contextMenuForElement elementInfo: WKContextMenuElementInfo,
    willCommitWithAnimator animator: UIContextMenuInteractionCommitAnimating
  ) {
    guard let chat = Self.chat(elementInfo.linkURL) else { return }
    animator.preferredCommitStyle = .dismiss
    animator.addCompletion { [weak self] in self?.onCommit?(chat) }
  }
}

/// The chat's card, as the page drew it, over the menu.
final class CardPreview: UIViewController {
  private let image: UIImage

  init(image: UIImage) {
    self.image = image
    super.init(nibName: nil, bundle: nil)
    preferredContentSize = image.size
  }

  required init?(coder: NSCoder) { nil }

  override func loadView() {
    let view = UIImageView(image: image)
    view.contentMode = .scaleAspectFill
    self.view = view
  }
}

@_cdecl("init_plugin_native_tabbar")
public func initPlugin() -> Plugin {
  return NativeTabbarPlugin()
}
