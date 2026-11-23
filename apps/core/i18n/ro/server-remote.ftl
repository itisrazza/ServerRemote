## Fereastră

tab-connect = Conectare la…
tab-manager = Manager de conexiuni
tab-settings = Setări
connection-closed = Conexiune închisă

## Butoane comune

button-ok = OK
button-cancel = Anulează
button-close = Închide

## Dialoguri

error-open-link = Linkul nu a putut fi deschis
connection-failed-title = Conectarea a eșuat
connection-failed-unknown = Conexiunea s-a închis înainte de a fi stabilită.
copy-details = Copiază detaliile
quit-title = Ieșire?
quit-message =
    { $count ->
        [one] Conexiunea deschisă va fi închisă.
       *[other] Toate conexiunile deschise vor fi închise.
    }
quit-confirm = Ieșire
licences-title = Licențe terțe
licences-icons = Pictograme: Carbon Design System de la IBM, sub Licența Apache 2.0.
licences-font = Font: Nunito Sans de la autorii proiectului Nunito Sans, sub licența SIL Open Font License 1.1.
recording-folder-dialog-title = Salvează înregistrările în
file-dialog-supported = Fișiere acceptate

## Fila Conectare

connect-name = Nume
connect-hostname = Gazdă
connect-hostname-placeholder = example.com
connect-hostname-placeholder-port = example.com:{ $port }
connect-username = Utilizator
connect-username-placeholder = Administrator
connect-password = Parolă
connect-save = Salvează
connect-save-tooltip = Salvează în managerul de conexiuni
connect-save-title = Salvează conexiunea
connect-connect = Conectează

## Managerul de conexiuni

manager-empty = Nicio conexiune salvată încă. Adăugați una sau importați un fișier.
manager-all = Toate ({ $count })
manager-group = { $group } ({ $count })
manager-add = Adaugă
manager-import = Importă
manager-edit = Editează…
manager-export = Exportă…
manager-delete = Șterge…
manager-group-menu = Grup
manager-group-none = Niciunul
manager-new-group = Grup nou…
manager-new-group-title = Grup nou
manager-group-name = Numele grupului
manager-new-group-create = Creează
manager-add-title = Adaugă o conexiune
manager-edit-title = Editează conexiunea
manager-delete-title = Ștergeți { $name }?
manager-delete-message = Conexiunea salvată va fi eliminată definitiv.
manager-delete-confirm = Șterge
manager-import-dialog-title = Importă conexiuni
manager-export-dialog-title = Exportă conexiunea
manager-file-type = Conexiuni ServerRemote
manager-import-failed = Conexiunile nu au putut fi importate
manager-export-failed = Conexiunea nu a putut fi exportată

## Conexiune

status-connecting = Se conectează
status-connecting-ellipsis = Se conectează…
status-connected = Conectat
status-disconnected = Deconectat
close-tab = Închide fila
waiting-for-frame = Se așteaptă primul cadru…
certificate-title = Certificat neverificat
certificate-message =
    { $host } a prezentat un certificat care nu poate fi verificat. Poate fi autosemnat sau expirat. Continuă doar dacă amprenta se potrivește cu cea la care te aștepți:

    { $fingerprint }
certificate-block = Blochează
certificate-connect-once = Acceptă o dată
certificate-always-trust = Acceptă mereu
certificate-blocked = Ai blocat certificatul acestui server. Deblochează-l din Setări › Securitate ca să te conectezi.
certificate-refused-strict = Certificatul serverului nu e emis de o autoritate recunoscută, iar verificarea certificatelor e setată pe Strictă.
certificate-not-trusted = Certificatul serverului nu a fost acceptat.
info-protocol = Protocol
info-address = Adresă
info-status = Stare
info-resolution = Rezoluție
info-pointer = Indicator
info-frames = Cadre primite
info-encryption = Criptare
info-title = Detaliile conexiunii
info-received = Primit
info-sent = Trimis
pointer-absolute = Absolut
pointer-relative = Relativ
encryption-none = Niciuna
menu-virtual-keyboard = Tastatură virtuală
menu-virtual-trackpad = Trackpad virtual
menu-input-settings = Setări de intrare…
menu-zoom-out = Micșorează
menu-zoom-in = Mărește
menu-display-settings = Setări de afișare…
menu-start-recording = Începe înregistrarea
menu-recording-settings = Setări de înregistrare…
trackpad-left = Stânga
trackpad-middle = Mijloc
trackpad-right = Dreapta

## Setări

section-general = General
section-display = Afișare
section-input = Intrare
section-recording = Înregistrare
section-security = Securitate
section-about = Despre

setting-confirm-quit = Confirmă înainte de ieșire
setting-confirm-quit-description = Întreabă înainte de a închide fereastra cât timp există conexiuni deschise.
setting-appearance = Aspect
setting-appearance-description = Culori deschise sau închise.
setting-language = Limbă
setting-graphics-backend = Backend grafic
setting-graphics-backend-description = DirectX 12 ține pasul mai bine cu redimensionarea ferestrei; încercați Vulkan dacă desenarea nu funcționează corect. Se aplică la următoarea deschidere a aplicației.
setting-scaling = Scalare
setting-scaling-description = Cum se încadrează ecranul la distanță în fereastră.
setting-keep-aspect-ratio = Păstrează proporțiile
setting-keep-aspect-ratio-description = Adaugă benzi negre în loc să întindă ecranul la distanță.
setting-display-scale = Scară de afișare
setting-display-scale-description = Cum corespund pixelii de la distanță celor ai acestui ecran.
setting-keyboard-style = Stil tastatură
setting-keyboard-style-description = Aspectul tastaturii virtuale la deschiderea unei conexiuni.
setting-keyboard-on-connect = Arată tastatura virtuală la conectare
setting-keyboard-on-connect-description = Deschide tastatura virtuală imediat ce se deschide o conexiune.
setting-keyboard-layout = Aranjament tastatură
setting-keyboard-layout-description = Aranjamentul tastaturii calculatorului de la distanță.
setting-recording-folder = Salvează în
setting-recording-folder-description = Unde se salvează înregistrările noi.
setting-recording-format = Format
setting-recording-format-description = Înregistrările ServerRemote păstrează și intrările de la tastatură și mouse.
setting-recording-quality = Calitate
setting-recording-quality-description = O calitate mai mare produce fișiere mai mari.
setting-check-on-launch = Caută actualizări la pornire
setting-updates = Actualizări
setting-certificate-checking = Verificarea certificatelor
certificate-checking-strict = Strictă
certificate-checking-ask = Întreabă
certificate-checking-off = Dezactivată
certificate-checking-strict-description = Conectează-te doar la servere cu certificate emise de o autoritate recunoscută.
certificate-checking-ask-description = Întreabă înainte de a te conecta la un server cu un certificat necunoscut. Îl poți accepta o dată sau îl poți reține.
certificate-checking-off-description = Acceptă orice certificat fără să întrebe. Oricine din rețea se poate da drept server și îți poate vedea parolele.
trusted-certificates = Certificate de încredere
trusted-certificates-description = Aceste servere se conectează fără întrebări, cât timp prezintă același certificat.
blocked-certificates = Certificate blocate
blocked-certificates-description = Aceste certificate sunt refuzate întotdeauna.
certificates-none = Niciunul încă.
certificate-forget = Uită
certificate-unblock = Deblochează

appearance-system = Ca sistemul
appearance-light = Luminos
appearance-dark = Întunecat
language-system = Ca sistemul
scaling-fit = Scalează pentru încadrare
scaling-actual = Mărime reală
display-scale-automatic = Automat
keyboard-style-compact = Compactă
keyboard-style-tenkeyless = Fără bloc numeric
keyboard-style-full = Completă
keyboard-layout-us = SUA
keyboard-layout-uk = Marea Britanie
keyboard-layout-german = Germană
keyboard-layout-french = Franceză
quality-low = Scăzută
quality-medium = Medie
quality-high = Ridicată
quality-lossless = Fără pierderi
recording-folder-other = Altul…
recording-folder-videos = Videoclipuri

update-not-checked = Neverificat încă.
update-checking = Se verifică…
update-up-to-date = Aveți cea mai recentă versiune.
update-no-releases = Nu a fost publicată nicio versiune încă.
update-available = Versiunea { $version } este disponibilă.
update-failed = Nu s-au putut căuta actualizări: { $error }
update-check-now = Verifică acum
update-check-again = Verifică din nou
update-try-again = Încearcă din nou
update-download = Descarcă

## Despre

about-website = Site web
about-made-by = Realizat de:
about-licence = Licență
about-licence-description = ServerRemote este licențiat sub MIT sau Apache 2.0, la alegerea dumneavoastră.
about-mit-license = Licența MIT
about-apache-license = Licența Apache 2.0
about-third-party-licences = Licențe terțe
