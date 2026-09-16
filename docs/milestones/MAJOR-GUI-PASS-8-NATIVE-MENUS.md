# Major GUI Pass 8 — Native Menus

Linux gains an in-window GTK4 PopoverMenuBar backed by the same GAction/GMenu command model as the compact hamburger menu. The permanent categories are File, Edit, View, Go, Transfer, Window, and Help.

macOS keeps menus in the native system menu bar via NSApp.mainMenu. The menu set is expanded to Cyber-Pumpkin, File, Edit, View, Go, Transfer, Window, and Help, including standard Services, Hide, Quit, Cut/Copy/Paste/Select All, Close Window, Minimize, Zoom, and Bring All to Front behavior.

No distro-specific global-menu extension is required on Linux, and no in-window menu bar is added on macOS.
