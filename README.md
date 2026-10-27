# winscrub

כלי CLI בראסט שמנקה דפדפנים ואת Windows 11 מאותו מקום — עם מדידה לפני מחיקה,
רמות סיכון, ו־guard שמסרב לגעת בכל מה שלא נמצא ברשימת התיקיות המותרות.

## בנייה

```bash
cargo build --release
```

הבינארי יוצא ב־`target\release\winscrub.exe`.

## שימוש

```bash
winscrub scan
```

מודד בלבד, אף פעם לא מוחק. ברירת המחדל היא `--level safe` — רק cache ו־temp.

```bash
winscrub scan --level caution --browsers
```

```bash
winscrub clean --level safe --close-browsers
```

```bash
winscrub clean --only chrome.default.cache,system.temp.user -y
```

```bash
winscrub clean --level all --deep --elevate
```

```bash
winscrub clean --dry-run --level caution --json report.json
```

```bash
winscrub list --paths
```

```bash
winscrub doctor
```

`doctor` מדפיס את תיקיות הבסיס המותרות, את התיקיות המוגנות, את רשימת שמות
הקבצים שלעולם לא נמחקים, והאם התהליך רץ כמנהל.

## פקודות ודגלים

| פקודה | מה היא עושה |
|---|---|
| `scan` | מודדת כמה מקום אפשר לפנות. read only. |
| `clean` | מוחקת את המטרות שנבחרו, אחרי אישור. |
| `list` | מדפיסה את כל המטרות שקיימות במחשב הזה. |
| `doctor` | מציגה את תצורת הבטיחות. |

| דגל | משמעות |
|---|---|
| `--browsers` / `--system` / `--apps` | הגבלה לקבוצה. בלי אף אחד — הכול. |
| `--only ID[,ID]` | מטרות מסוימות לפי id, קידומת נקודה, או `*`. עוקף את `--level` ו־`--deep`. |
| `--skip ID[,ID]` | להוציא מטרות. |
| `--level safe\|caution\|all` | כמה סיכון מותר. ברירת מחדל `safe`. |
| `--deep` | כולל מטרות כבדות שדורשות הרשאות מנהל. |
| `--dry-run` | ריצה מלאה בלי מחיקה. |
| `-y, --yes` | בלי שאלת אישור. |
| `--close-browsers` | לבקש מהדפדפנים להיסגר קודם (יפה, ואז בכוח). |
| `--elevate` | הרצה מחדש דרך UAC כשצריך הרשאות מנהל. |
| `--json FILE` | דוח מכונה. |
| `--no-color`, `-v` | פלט בלי ANSI / פירוט שגיאות. |

## רמות סיכון

* **safe** — cache, temp, shader cache, crash dumps, לוגים. נבנה מחדש לבד, לא מאבדים כלום.
* **caution** — cookies (מתנתקים מאתרים), היסטוריה, טאבים שמורים, prefetch, קבצים אחרונים.
* **destructive** — סל המיחזור, יומני אירועים, `Windows.old`. אין דרך חזרה, ודורש להקליד `yes` מלא.

## מה מנוקה

**דפדפנים** — Chrome (כולל Beta/Canary), Edge, Edge WebView2, Brave, Vivaldi,
Chromium, Yandex, Arc, Opera, Opera GX, Firefox, LibreWolf, Waterfox, Zen, Tor.
לכל פרופיל בנפרד: HTTP cache, code cache, GPU/Dawn cache, service worker cache,
media cache, blob storage, crash reports — ובדרגת caution גם cookies, היסטוריה
וטאבים שמורים.

**Windows** — ‎`%TEMP%`, ‎`Windows\Temp`, ‎`SystemTemp`, INetCache, WebCache,
thumbnail ו־icon cache, crash dumps, דוחות שגיאה (WER) של המשתמש ושל המכונה,
קבצים אחרונים, cache של NVIDIA / AMD / Intel / DirectX, font cache, caches של
אפליקציות Store, cache של Windows Update, Delivery Optimization, לוגי CBS/DISM/Setup,
minidumps, ‎`Windows.old`, סל המיחזור, DNS cache, יומני אירועים, ו־WinSxS דרך DISM.

**אפליקציות** — Discord, Slack, Teams (הישן), Signal, WhatsApp, Obsidian,
VS Code, Spotify, Steam, Epic Games, Telegram, pip, npm, Yarn, NuGet.

## מודל הבטיחות

1. **רשימת בסיס סגורה.** מחיקה מותרת רק בתוך `%LOCALAPPDATA%`, `%APPDATA%`,
   `%TEMP%`, ותיקיות ספציפיות תחת `%WINDIR%` ו־`ProgramData\Microsoft\Windows\WER`.
   כל נתיב אחר נדחה, גם אם מטרה כלשהי מצביעה עליו.
2. **תיקיות מוגנות.** שורש הכונן, `Windows`, `System32`, `Program Files`,
   פרופיל המשתמש, שולחן העבודה, מסמכים, הורדות, תמונות, OneDrive ועוד — לא
   נמחקות, וגם לא נמחקת שום תיקייה שמכילה אותן.
3. **רשימת שמות שלעולם לא נמחקים** — `Login Data`, `Web Data`, `Local State`,
   `Bookmarks`, `places.sqlite`, `key4.db`, `logins.json`, `pagefile.sys`,
   `NTUSER.DAT` ועוד. סיסמאות, אמצעי תשלום וסימניות לא נוגעים בהם אף פעם.
4. **בלי symlinks ו־junctions.** לא נכנסים דרכם ולא מוחקים אותם, כדי ששום reparse
   point לא יוציא את המחיקה מהתיקייה שהתכוונו אליה.
5. **קובץ נעול זה לא כישלון.** קבצים שתפוסים בידי תהליך רץ נספרים כ־skipped.
6. **היסטוריה של Firefox לא מוצעת בכלל** — היא יושבת ב־`places.sqlite` יחד עם
   הסימניות, ואין דרך למחוק אחת בלי השנייה.

## קודי יציאה

`0` הצליח · `1` שגיאה · `2` חלק מהמטרות נכשלו

## מבנה

```
src/
  main.rs          תזרים הפקודות
  cli.rs           הגדרת ה־CLI
  model.rs         Target, Risk, Group, TargetKind
  safety.rs        ה־Guard: שורשים, תיקיות מוגנות, blocklist
  fsops.rs         מדידה, מחיקה, גדלים, glob
  engine.rs        בחירה, סריקה מקבילית, ניקוי
  actions.rs       shell, שירותים, סגירת אפליקציות
  report.rs        טבלאות, צבע, אישור, JSON
  winapi.rs        Win32: elevation, תהליכים, סל מיחזור, ANSI
  targets/
    browsers.rs    Chromium + Gecko
    system.rs      Windows
    apps.rs        אפליקציות צד שלישי
```
