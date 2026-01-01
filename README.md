# Labar (AI Generated Panel)

I couldn't find a beautiful panel with the functionality I needed for `labwc`, and since I don't know how to program, this entire panel was written with the help of AI. Because of this, it likely contains a bunch of errors.

I want anyone who needs it to use it however they want. If someone wants to improve the panel or make a fork, please feel free to do so.
![screenshot](labar.png)
## Installation / Usage

You can use the code entirely as you wish.

### Labwc Configuration

To use this panel with `labwc`, add the following to your configuration.

#### Autostart
Add the panel to your autostart script/config so it launches on startup.

#### Start Menu Keybinding
To open the start menu with the `Super` (Windows) key, add this keybind to your `labwc` configuration (usually `rc.xml` or `config.xml`):

```xml
<keybind key="Super_L">
  <action name="Execute">
    <command>pkill -SIGUSR1 -x labar</command>
  </action>
</keybind>
```
