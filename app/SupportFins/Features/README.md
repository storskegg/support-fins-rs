# Features

A feature lives in its own folder (`Rotate/`, `Suggest/`, ...) and plugs into the window; `ContentView`, `InspectorView`
and the menu bar compose what the features contribute, so a feature PR touches no shared file.

To add one:
1. Make `YourFeature: Feature` (`Features/Feature.swift`) in your folder: a `@MainActor final class` with
   `init(host: ViewerModel)`. Keep your models in the feature, not in `ViewerModel`; read the shared ones
   (`host.session`, `host.controller`, `host.wiring`, `host.toolbar`) from `host`.
2. Add one line to `Features.all` (`Features/Features.swift`). Swift has no reliable automatic registration, so this is
   the single list.
3. Implement only what you need; every property has an empty default.

What a feature may register (`Features/Contributions.swift`): `overlays` (alignment, `order`: higher is on top),
`modifiers` (sheets, drop targets, `WindowModifier.onChange`), `windowToolbarItems`, `toolbarSlots` plus
`registerTools(in:)` for the floating toolbar's badges (declare a `static let` `ToolbarSlot` of your own),
`inspectorSections` (slot `.top`, `.basics` or `.bottom`), `statusLines` (the status card), `escapeHandlers`
(priority: higher first; return true to take the key and stop the cascade), `partWillShow` / `partDidShow`, and
`static func menus(_:)` (`MenuSection`s for `.view`, `.model`, `.fileExport`, or your own `MenuID`; items come
in section `order`, so a feature's items sit where the menu has them).

Ties in `order` keep the `Features.all` order. A menu of your own is shown by `FeatureCommands`, which has room for three.
`FeatureSetTests` pins the overlay, menu-item and Esc orders; a change to a menu changes its snapshot there.
