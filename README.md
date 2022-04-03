# Energia

Energia is a simple and configurable power manager for Linux. A *power manager*
is a tool which detects when your system is idle, that is, when you're not using
your keyboard or mouse and you're not running any applications which prevent
your system from being idle (such as video players). Once your system is idle,
it runs a series of actions (called *effects* in Energia) after configurable
time has passed.

## Requirements and installation

Currently, your system must have these 3 components installed and in use:

* `systemd` - used to put computer to sleep, change screen brightness without
  being root, detect inhibitors (applications which prevent your computer from
  going into an idle state or from sleeping) and announce idleness and locking
  to the rest of the system.

* `X11` - the display server announces the idleness and handles screen shutdowns

* `upower` - used to detect the system's power source

Since Energia has a highly modular codebase, most of these can be replaced or
adapted (to use e.g. Wayland or phase out `upower`) by anyone who is at least a
bit skilled in Rust. MRs are welcome!

You may find that you want two more things to be installed:

* A *locker* which can be invoked as a CLI command (such as
  [i3lock](https://github.com/i3/i3lock)) is necessary to use the screen locking
  functionality in Energia. This is not integrated, since different people may
  want to use different lockers to suit their needs and aesthetic preferences.

* An application which can handle power saving settings configuration, such as
  [TLP](https://linrunner.de/tlp/). Because TLP is, as they say themselves and
  our experience confirms, an "install and forget" kind of application, we have
  decided not to duplicate their work and instead recommend this wonderful
  package.

For now, Energia is not distributed as a package. Since it's written in Rust,
you can compile it yourself with a simple `cargo build` command and then plop it
down on your system and run it as a systemd user service. A sample unit file is
coming soon. 

For more information about installing a Rust toolchain, please refer to [their
documentation](https://www.rust-lang.org/learn/get-started). Energia or its
dependencies don't use unstable Rust features, so installing a stable toolchain
is sufficient.

## Glossary

Before we get into the details of configuration, we need to define some terms
which are used in the rest of the documentation:

* **Effect** is an action that Energia performs on your system. An example would
  be dimming the screen, locking your computer or putting it to sleep. Effects
  are executed at a time defined by the schedule and most are rolled back once
  you wake your system (for example by moving your mouse or pressing a key).
  Some effects are obviously not rolled back automatically, such as system
  locking (since you probably don't want to unlock your computer by pressing any
  key) or sleep ("rollback" of that is handled by your PC).

* **Effector** is a module which provides effects. It can provide a single
  effect or multiple effects which have to be executed sequentially. All effects
  from an effector share a configuration section.

* **Schedule** specifies the periods of idleness after which certain effects are
  performed. You can for example say that you want to dim the screen after 3
  minutes and turn it off after 10 minutes of inactivity. Energia will then wait
  until 3 minutes have passed since you've last interacted with your computer
  and if no application will be inhibiting system idleness, it will dim the
  screen. If you don't interact with your computer for further 7 minutes, it
  will turn the screen off.

## Anatomy of a configuration file

Energia's configuration is written in [TOML](https://toml.io) and at the
minimum, must contain at least one schedule and configurations required for
effectors which provide effects used in your schedules.

An example configuration file would look like this:

```toml
# Schedule to follow when the PC is running with an external power source
[schedule.external]
# After 3 minutes of inactivity, the screen will dim
# and the system will be locked
screen_dim  = "3m"
lock        = "3m"
# After further 30 seconds, the screen will be turned off
screen_off  = "3m 30s"
# And once the computer has been idle for 10 minutes 
#(i.e. 6:30 minutes after the screen gets turned off),
# it will be put to sleep
sleep       = "10m"

# Schedule to follow when the PC is running on battery
[schedule.battery]
lock        = "5m"
screen_dim  = "10m"
screen_off  = "15m"

# Configuration for lock effector
[lock]
command = "i3lock"
args = ["-n"] # Ensure i3lock will not fork, thus allowing Energia to know whether it should start a new locker or not.
```

The times in the schedules are specified as **absolute** times within the
idleness period.

## Runtime configuration

There are two environment variables that can be set to control Energia's
behavior:

* `ENERGIA_CONFIG_PATH` which sets the path to the configuration file described
  above. By default, Energia will look for `config.toml` in the current
  directory.
* `RUST_LOG` which sets the log verbosity. Available levels are `error`, `warn`,
  `info`, `debug` and `trace`. Now during development, the default value is
  `debug`.

## A list of effectors, provided effects and configurations

* **brightness** effector
    * Provided effects:
        * `screen_dim` - dim the screen to 50% of its current brightness.
    * Configuration:
        * `dim_percentage` (integer, default: 50) - the percentage to which the brightness should be
          reduced relative to the current brightness.
* **dpms** effector
    * Provided effects:
        * `screen_off` - turn all the screens connected to the computer off.
    * Configuration:
        * N/A
* **lock** effector
    * Provided effects:
        * `lock` - start a screen locking application and set `LockedHint` on
          user's `logind` session to `true`. Never rolled back automatically.
    * Configuration:
        * `command` (string, required) - the path to the locker to execute.
        * `args` (list of strings, required) - arguments to be passed to the locker.
    * If configuring this effector will cause additional features to be enabled,
      see [below](#additional-locking-behavior).
* **sleep** effector
    * Provided effects:
        * `sleep` - put the computer to sleep as if by calling `systemd suspend`
          on the command line.
    * Configuration:
        * N/A
* **session** effector
    * Provided effects:
        * `idle_hint` - set the `IdleHint` property on user's `logind` session
          to `true`. This effect is only mentioned for completeness. Energia
          automatically executes it with the first real effect specified in the
          schedule.
    * Configuration:
        * N/A

## Additional locking behavior

If you configure the lock effector, two additional features will be enabled,
even if `lock` action is not in any schedule:

* **Automatic locking on sleep** - When Energia detects that your computer is
going to sleep, it will invoke the locker.

* **D-Bus lock invocation API** - You can lock your computer by sending a Lock
  message on session/user D-Bus. The service is `org.energia.Manager`, path is
  `/org/energia/Manager` and the interface is `org.energia.Manager`.

  This can be used in conjunction with `busctl` to allow hotkey-triggered
  locking. For example, if you want to lock your session with a Modifier+Shift+L
  hotkey in i3, you can add the following to your i3 config:

  ```
  bindsym $mod+Shift+l exec busctl --user call org.energia.Manager /org/energia/Manager org.energia.Manager Lock
  ```
