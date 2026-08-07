Layout
> **Agent application directive — premium UI:** Make the primary task and content immediately discoverable through hierarchy, alignment, grouping, and progressive disclosure. Use adaptive grids and system safe areas; preserve relationships through resize, orientation, text scaling, localisation, and narrow widths. Do not crowd unrelated controls or rely on fixed positioning where a flexible layout is required.
A consistent layout that adapts to various contexts makes your experience more approachable and helps people enjoy their favorite apps and games on all their devices.
A sketch of a small rectangle in the upper-left quadrant of a larger rectangle, suggesting the position of a user interface element within a window. The image is overlaid with rectangular and circular grid lines and is tinted yellow to subtly reflect the yellow in the original six-color Apple logo.

Your app’s layout helps ground people in your content from the moment they open it. People expect familiar relationships between controls and content to help them use and discover your app’s features, and designing the layout to take advantage of this makes your app feel at home on the platform.

Apple provides templates, guides, and other resources that can help you integrate Apple technologies and design your apps and games to run on all Apple platforms. See Apple Design Resources.

Best practices
Group related items to help people find the information they want. For example, you might use negative space, background shapes, colors, materials, or separator lines to show when elements are related and to separate information into distinct areas. When you do so, ensure that content and controls remain clearly distinct.

Make essential information easy to find by giving it sufficient space. People want to view the most important information right away, so don’t obscure it by crowding it with nonessential details. You can make secondary information available in other parts of the window, or include it in an additional view.

Extend content to fill the screen or window. Make sure backgrounds and full-screen artwork extend to the edges of the display. Also ensure that scrollable layouts continue all the way to the bottom and the sides of the device screen. Controls and navigation components like sidebars and tab bars appear on top of content rather than on the same plane, so it’s important for your layout to take this into account.

When your content doesn’t span the full window, use a background extension view to provide the appearance of content behind the control layer on either side of the screen, such as beneath the sidebar or inspector. For developer guidance, see backgroundExtensionEffect() and UIBackgroundExtensionView.

A screenshot of a full screen iPad app with a sidebar on the leading edge. A photo of Mount Fuji fills the top half of the content area. The photo subtly blurs as it reaches the top of the screen, where toolbar items float above it grouped on the trailing edge. Where the photo meets the sidebar, the image flips, blurs, and extends fully beneath the sidebar to the edge of the screen.

Visual hierarchy
Differentiate controls from content. Take advantage of the Liquid Glass material to provide a distinct appearance for controls that’s consistent across iOS, iPadOS, and macOS. Instead of a background, use a scroll edge effect to provide a transition between content and the control area. For guidance, see Scroll views.

Place items to convey their relative importance. People often start by viewing items in reading order — that is, from top to bottom and from the leading to trailing side — so it generally works well to place the most important items near the top and leading side of the window, display, or field of view. Be aware that reading order varies by language, and take right to left languages into account as you design.

Align components with one another to make them easier to scan and to communicate organization and hierarchy. Alignment makes an app look neat and organized and can help people track content while scrolling or moving their eyes, making it easier to find information. Along with indentation, alignment can also help people understand an information hierarchy.

Take advantage of progressive disclosure to help people discover content that’s currently hidden. For example, if you can’t display all the items in a large collection at once, you need to indicate that there are additional items that aren’t currently visible. Depending on the platform, you might use a disclosure control, or display parts of items to hint that people can reveal additional content by interacting with the view, such as by scrolling.

Make controls easier to use by providing enough space around them and grouping them in logical sections. If unrelated controls are too close together — or if other content crowds them — they can be difficult for people to tell apart or understand what they do, which can make your app or game hard to use. For guidance, see Toolbars.

Adaptability
Every app and game needs to adapt when the device or system context changes. In iOS, iPadOS, tvOS, and visionOS, the system defines a collection of traits that characterize variations in the device environment that can affect the way your app or game looks. Using SwiftUI or Auto Layout can help you ensure that your interface adapts dynamically to these traits and other context changes; if you don’t use these tools, you need to use alternative methods to do the work.

Here are some of the most common device and system variations you need to handle:

Different device screen sizes, resolutions, and color spaces

Different device orientations (portrait/landscape)

System features like Dynamic Island and camera controls

External display support, Display Zoom, and resizable windows on iPad

Dynamic Type text-size changes

Locale-based internationalization features like left-to-right/right-to-left layout direction, date/time/number formatting, font variation, and text length

Design a layout that adapts gracefully to context changes while remaining recognizably consistent. People expect your experience to work well and remain familiar when they rotate their device, resize a window, add another display, or switch to a different device. You can help ensure an adaptable interface by respecting system-defined safe areas, margins, and guides (where available) and specifying layout modifiers to fine-tune the placement of views in your interface.

Be prepared for text-size changes. People appreciate apps and games that respond when they choose a different text size. When you support Dynamic Type — a feature that lets people choose the size of visible text in iOS, iPadOS, tvOS, visionOS, and watchOS — your app or game can respond appropriately when people adjust text size. To support Dynamic Type in your Unity-based game, use Apple’s accessibility plug-in (for developer guidance, see Apple – Accessibility). For guidance on displaying text in your app, see Typography.

Preview your app on multiple devices, using different orientations, localizations, and text sizes. You can streamline the testing process by first testing versions of your experience that use the largest and the smallest layouts. Although it’s generally best to preview features like wide-gamut color on actual devices, you can test on a simulated device in Device Hub to check for clipping and other layout issues. For example, if your iOS app or game supports landscape mode, you can use the simulator to make sure your layouts look great whether the device rotates left or right.

When necessary, scale artwork in response to display changes. For example, viewing your app or game in a different context — such as on a screen with a different aspect ratio — might make your artwork appear cropped, letterboxed, or pillarboxed. If this happens, don’t change the aspect ratio of the artwork; instead, scale it so that important visual content remains visible. In visionOS, the system automatically scales a window when it moves along the z-axis.

Guides and safe areas
A layout guide defines a rectangular region that helps you position, align, and space your content on the screen. The system includes predefined layout guides that make it easy to apply standard margins around content and restrict the width of text for optimal readability. You can also define custom layout guides. For developer guidance, see UILayoutGuide and NSLayoutGuide.

A safe area defines the area within a view that isn’t covered by a toolbar, tab bar, or other views a window might provide. Safe areas are essential for avoiding a device’s interactive and display features, like Dynamic Island on iPhone or the camera housing on some Mac models. For developer guidance, see SafeAreaRegions and Positioning content relative to the safe area.

Respect key display and system features in each platform. When an app or game doesn’t accommodate such features, it doesn’t feel at home in the platform and may be harder for people to use. In addition to helping you avoid display and system features, safe areas can also help you account for interactive components like bars, dynamically repositioning content when sizes change.

For templates that include the guides and safe areas for each platform, see Apple Design Resources.

## Agent context engineering

- **Seven-piece stack:** Use this file as **Instructions**; pair it with the current **User Input**, selected **Retrieved Facts**, allowed **Tools**, concise **Short-term Notes**, stable project **Long-term Memory**, and an explicit layout-plan **Output Format**.

- **Select:** Retrieve the target screen, viewport/breakpoint, content priority, safe-area constraints, and affected components before changing layout.
- **Write/compress:** Record agreed information hierarchy, grid, breakpoint, and collapse behaviour as stable facts; keep only active layout experiments in the scratchpad.
- **Isolate/output:** Separate layout reasoning from visual styling. Output hierarchy, responsive rule, safe-area behaviour, and empty/long-content handling.
