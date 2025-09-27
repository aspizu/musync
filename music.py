import argparse
import html
import itertools
import json
import re
import sys
from pathlib import Path

from mutagen.easyid3 import EasyID3
from mutagen.flac import FLAC, FLACNoHeaderError
from mutagen.mp3 import MP3, HeaderNotFoundError

BOLD = "\033[1m"
GREEN = "\033[0;32m"
RED = "\033[0;31m"
CYAN = "\033[0;36m"
YELLOW = "\033[1;33m"
RESET = "\033[0m"


def ask(prompt: str):
    print(f"{prompt} (Y/N)", end=" ", flush=True)

    def get_keypress():
        if sys.platform == "win32":
            import msvcrt

            return msvcrt.getch().decode("utf-8").lower()
        else:
            import termios
            import tty

            fd = sys.stdin.fileno()
            old_settings = termios.tcgetattr(fd)
            try:
                tty.setraw(fd)
                key = sys.stdin.read(1).lower()
            finally:
                termios.tcsetattr(fd, termios.TCSADRAIN, old_settings)
            return key

    while True:
        key = get_keypress()
        if key == "\x03":
            sys.exit(1)
        if key == "y":
            print()
            return True
        elif key == "n":
            print()
            return False


argparse = argparse.ArgumentParser()
argparse.add_argument("path", nargs="?", default=".")
argparse.add_argument("--log", default="log.txt")
argparse.add_argument("--rejects", default="rejects.txt")
argparse.add_argument("--undo", action="store_true")
argparse.add_argument("-v", "--verbose", action="store_true")
args = argparse.parse_args()
if args.undo:
    log = Path(args.log).read_text().splitlines()
    if len(log) == 0 or log[-1] == "":
        print("No actions to undo")
        sys.exit(0)
    action = json.loads(log.pop())
    if action["action"] == "rename":
        Path(action["new"]).rename(action["old"])
        print(f"{RED}{action['new']}{RESET} renamed to {GREEN}{action['old']}{RESET}")
        Path(args.log).write_text("\n".join(log) + "\n")
    sys.exit(0)
logfile = Path(args.log).open("a")
rejectsfile = Path(args.rejects).open("a")
rejects = Path(args.rejects).read_text().splitlines()
path = Path(args.path)

sites = [
    "pagalworld.com(.[a-z]+)?",
    "www.Songs.PK",
    "DJMaza.Info",
    "www.\\w+.Com",
    "® Riya collections ®",
    "MyMp3Song.Com",
    "DownloadMing.SE",
    "PagalWorld.me",
    "www.Songspk.name",
    "WwW.AlluArjunS.Tk",
    "MahaMP3.Com",
    "www.FreshMaza.Info",
    "PagalNew",
    "@ Songgy.net",
    "RoyalJatt.Com",
    "PaglaSongs.Com",
    "PaglaSongs",
    "PagalSongs.Com.iN",
    "PagalNew.Com.Se",
    "RiskyjaTT.CoM",
    "PagaliWorld.Com",
    "PagalWorld",
    "PagalSongs.com",
    "DjPunjab.CoM",
    "\\(Full Song\\)",
    "Djjohal.fm",
    "\\(Original Mix\\)",
    "\\(Raag.Fm\\)",
    "Mr-Jat.in"
]
sitesre = re.compile(
    r"\s*-?[-|;\[(]?\s*("
    + "|".join([site.replace(".", "\\.") for site in sites])
    + r")\s*[-|;\])]?\s*",
    re.IGNORECASE,
)


def clean_up_title(title: str) -> str:
    title = sitesre.sub("", title)
    title = title.title().strip()
    return html.unescape(title)


def clean_up_artist(artist: str) -> str:
    artist = sitesre.sub("", artist)
    artist = ", ".join(set(artist.strip() for artist in artist.split(",")))
    return html.unescape(artist)


for subpath in itertools.chain(path.glob("**/*.mp3"), path.glob("**/*.flac")):
    if subpath.absolute().as_posix() in rejects:
        continue
    if subpath.suffix == ".flac":
        try:
            file = FLAC(subpath)
        except FLACNoHeaderError:
            print(f"{YELLOW}{subpath.name}{RESET} is not a valid flac file")
            continue
    else:
        try:
            file = MP3(subpath, ID3=EasyID3)
        except HeaderNotFoundError:
            print(f"{YELLOW}{subpath.name}{RESET} is not a valid mp3 file")
            continue
    if args.verbose:
        print(file)
    title: str = subpath.stem
    if "title" in file:
        title: str = sitesre.sub("", file["title"][0])
    artist: str | None = None
    if "albumartist" in file:
        artist = sitesre.sub("", file["albumartist"][0])
    if "artist" in file and (
        artist == "Various Artists"
        or artist == ""
        or artist is None
        or (artist is not None and len(artist) < len(", ".join(file["artist"])))
    ):
        albumartist = artist
        artist = ", ".join(file["artist"])
        if (
            albumartist is not None
            and albumartist.lower() not in artist.lower()
            and albumartist != "Various Artists"
            and albumartist != ""
        ):
            artist = albumartist + ", " + artist
    title = html.unescape(sitesre.sub("", title).strip())
    if artist:
        artist = html.unescape(sitesre.sub("", artist).strip())
    filename = (
        f"{title} - {artist}{subpath.suffix}"
        if artist and artist not in title
        else f"{title}{subpath.suffix}"
    )
    filename = re.sub(r"[:|\\/?\"<>*]", " ", filename)
    filename = " ".join(re.split(r"\s+", filename))
    if subpath.name != filename:
        newpath = subpath.parent.joinpath(filename)
        if newpath.exists():
            print(f"{YELLOW}{subpath.name}{RESET} already exists")
            continue
        if ask(
            f"Rename {BOLD}{RED}{subpath.name}{RESET} to {BOLD}{GREEN}{filename}{RESET}?"
        ):
            subpath.rename(newpath)
            json.dump(
                {
                    "action": "rename",
                    "old": subpath.absolute().as_posix(),
                    "new": newpath.absolute().as_posix(),
                },
                logfile,
            )
            logfile.write("\n")
        else:
            rejectsfile.write(f"{subpath.absolute().as_posix()}\n")
