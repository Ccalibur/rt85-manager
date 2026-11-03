app-title = RT85 Manager

page-lighting = Lighting
page-keys = Keys
page-macros = Macros
page-screen = Screen
page-device = Device
page-settings = Settings

# Status
no-device = No supported keyboard connected
no-device-help = Plug the keyboard in over USB. If it still doesn't show up, install the udev rule (see README) so your user can open the HID device, then replug it.
error = Error
dismiss = Dismiss
apply = Apply
yes = Yes
no = No

# Lighting
effect = Effect
mode = Mode
speed = Speed
brightness = Brightness
direction = Direction
rainbow = Rainbow colours
color = Colour
applying-lighting = Applying lighting…
unsupported-lighting = This keyboard's lighting protocol isn't supported yet.

# Keys
tab-colors = Per-key colours
tab-remap = Remapping
select-all = Select all
select-none = Select none
paint-selected = Paint selected
send-colors = Send to keyboard
sending-colors = Sending key colours…
pick-a-key = Click a key to remap it
sends = Sends
apply-remap = Remap key
remapping = Remapping…
layout-note = Layout: { $name }. RT85 key indices are still being mapped, so some keys may be off.
unsupported-keys = Per-key colours and remapping aren't supported on this keyboard yet.

# Screen
image = Image
screen-size = Screen size
no-file = No file chosen
choose = Choose…
choose-image = Choose an image or GIF
fit = Fit
fit-cover = Fill (crop)
fit-contain = Fit (letterbox)
fit-stretch = Stretch
preparing = Preparing…
upload = Upload to screen
uploading = Uploading…
clear-screen = Clear screen
clearing = Clearing screen…
wrong-format = This keyboard can't take that file format (RDMCTMZT screens need .qgif).
widgets = Widgets
clock = Clock
sync-now = Sync now
syncing-time = Syncing clock…
cpu-feed = Show CPU usage
temp-feed = Show temperature
unsupported-screen = Uploading to this keyboard's screen isn't supported yet.
live-screen = Live screen
live-show = Show
live-show-help = Draws what's playing and/or the game's FPS and sends it to the screen as an image
live-off = Off
live-media = Now playing
live-fps = Game FPS
live-both = Now playing + FPS
live-refresh = Refresh time bar and FPS
live-refresh-help = Track changes are sent straight away. Every refresh re-sends the whole image, so keep this slow
refresh-track = Only on track change
refresh-every = Every { $secs } s
now-playing = Now playing
playing = playing
paused = paused
no-media = No media player is playing anything
game-fps = Game FPS
game-fps-help = Read from MangoHud while a game runs with it
mangohud-missing = MangoHud isn't installed
mangohud-setup = Turn on MangoHud logging
no-game = No game running with MangoHud
fps-gauge = Show FPS on the CPU gauge
fps-gauge-help = Updates every 2 s without re-sending an image. Shows at most 100; the CPU reading returns when no game is running
live-preview = Preview
live-unsupported = This keyboard's screen only takes pre-made .qgif files, so it can't show live content.

# Device
device = Keyboard
details = Details
platform = Platform
model = Model
manufacturer = Manufacturer
connection = Connection
interfaces = HID interfaces
rescan = Rescan
unknown-model = This VID:PID isn't in the known list yet. The protocol is guessed from the vendor ID, so please test carefully and report the IDs.

# Keyboard layout
keyboard = Keyboard
keyboard-layout = Layout
keyboard-layout-help = Used for the key map on the Keys page and the remap list
layout-ansi = US ANSI
layout-iso = UK ISO
layout-ansi-hint = Wide, single-row Enter key, \ above it, long left Shift
layout-iso-hint = Tall Enter key with # beside it, short left Shift with an extra \ key
layout-question = Which layout is your keyboard?
layout-question-body = Check the shape of the Enter key and left Shift. You can change this later in Settings.
continue = Continue

# Settings
appearance = Appearance
color-scheme = Colour scheme
scheme-system = Follow system
scheme-light = Light
scheme-dark = Dark
source-cosmic = Using the COSMIC theme
source-kde = Using the KDE colour scheme “{ $name }”
source-portal = Following the desktop's light/dark preference
source-builtin = Using the built-in theme
frosted-glass = Frosted glass
frosted-help = Blur the window background, like COSMIC's frosted-glass windows
frosted-help-other = Blur the window background. Needs a compositor with background blur support (COSMIC or KDE Plasma); GNOME and X11 sessions stay opaque

# Macros
macros = Macros
macros-enabled = Run macros
macros-enabled-help = While RT85 Manager is open, pressing a macro's key types its steps. The keyboard's own macro memory isn't supported yet, so macros only work while the app runs
macros-status = Status
macros-listening = Listening for macro keys
macros-unsupported = This keyboard's key remapping isn't supported yet, so macro keys can't be assigned.
no-key-interface = Couldn't find the keyboard's key input (is the udev rule installed?)
macro-default-name = Macro { $n }
new-macro = New macro
delete-macro = Delete
no-macros = No macros yet
not-assigned = No key assigned
step-count = { $n ->
    [one] 1 step
   *[other] { $n } steps
}
macro-edit = Edit macro
macro-name = Name
macro-trigger = Trigger code
macro-trigger-help = The unused key code the keyboard sends for this macro
macro-key = Key
assign-key = Assign key…
restore-key = Restore key
cancel = Cancel
macro-connect = Connect the keyboard to assign a key.
pick-macro-key = Click the key that should run this macro:
macro-steps = Steps
no-steps = No steps yet
step-key = Press key
step-text = Type text
step-delay = Wait
step-press = Press
step-type = Type
step-wait = Wait
text-to-type = Text to type
add-step = Add step
text-layout-note = Text is typed for a { $layout } keyboard layout; set the same layout in your desktop settings.
test-macro = Test (types in 3 s)
macro-testing = Typing “{ $name }” in 3 seconds — click where it should go
macro-played = Ran “{ $name }”
uinput-help = macros need write access to /dev/uinput
