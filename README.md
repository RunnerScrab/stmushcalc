# stmushcalc (TrekMUSH Ship Calculator)
This is a ship calculator for ATS TrekMUSH, which runs a simulator for a ship
or a list of ships (by name, comma separated) you give it and outputs a damage
plot.

It can be used on the command prompt, or through a World Wide Web application
program as an interactive, multimedia document.

## CLI plot generator
This is the command-line tool I first wrote to generate damage plots for
different ships.

- If you give it a single ship name, it will give you a plot of that ship's
  instantaneous damage output over time 
- If you give it multiple ship names, it will give a plot of each ship's
  cumulative damage output over time, plotted together so you can compare them

### Usage
```
shipdb <name>[,<name>...] [path/to/ships.db]
             [--svg --png] [--eng N --tac N --helm N --oper N --sci N --dam N --wis N]
```

To build the catalog or add to the existing one from a @listspecs log, instead
of plotting:

`shipdb --parse listspecs.log [--sqlite | --bincode]`
 
## Web calculator
This can also function as a web-based calculator, which you may visit at
https://runnerscrab.github.io/stmushcalc/. Most people would prefer this,
as it is easier to use and does everything the CLI tool does and more.

Thanks to the magic of WASM, it uses much of the same Rust code the CLI tool
uses (though is compiled to WASM rather than your CPU's machine code).

### Serving the app locally
The web-app requires a browser to run, but none of its features require an
internet connection to use, and it can be served and run locally.

You can run it locally by downloading the source and serving the `docs/`
directory, which I have prebuilt for deployment.

If you have Python 3 installed, an easy way to run it locally is: 

```
cd docs/
python3 -m http.server 8000
```

then opening the URL it shows you.

### Usage
You can load log files with @listspecs sheets in them through the system file
dialog (using the button) or by dragging them onto the page. You can load one
or more files at a time. Each file may contain one or more @listspecs sheets.

Everything not a @listspecs sheet for a ship class will be ignored, including
the @listspecs of your specific, tuned ship.

You can either make a text file with only @listspecs output in it, or just have
the program scrape them from your ordinary log files. The former will allow it
to parse the ships out more efficiently.

_No data is uploaded anywhere_, and is only stored locally by your browser so
you don't have to re-load all your ships every time you refresh the page.

## Ships included
_This tool only includes Independent faction ships_, (which everyone can see)
so you can demo how it works, but you can load those of your own faction if you
have their @listspecs sheets.

I don't know the exact rules concerning this, but to be safe you should
probably not distribute your faction's ship specs.

## Building

The CLI tool you can just build using
`RUSTFLAGS="-C target-cpu=native" cargo build --release`.

### Web app
You need the cargo and Rust toolchains installed, as well as wasm-pack and
wasm-bindgen to build the web application.

Installing them on my system (and hopefully yours) looks something like this:
```
rustup target add wasm32-unknown-unknown
cargo install wasm-pack
cargo install wasm-bindgen-cli --version 0.2.126
```

You would then run `build.sh`, which is an ad-hoc script but may work on your system.
(It needs the tools in your path, which it assumes is at `~/.cargo/bin`.)

## Note
(I named my world file for this game "STMush" and had mistakenly come to believe
that it was the game's actual name, hence why this is called stmushcalc.)

