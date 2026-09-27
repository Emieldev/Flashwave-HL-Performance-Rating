# Callouts traced from community callout images (Java59's series) into the
# app's callout files (Q28). Each map: landmark pairs between the callout
# image (as displayed, 2000 px wide) and the app's 1440 px overview give an
# affine transform; zones are traced on the callout image; mirrored maps
# repeat one side across the middle. Then every zone is checked against the
# stored kill positions.
#   python scripts/trace_callouts.py <a copy of the database> [map ...]
# Writes callouts/<map>.json. Never point it at the live database.
import json, os, sqlite3, sys
from collections import Counter

os.chdir(r"C:\Users\Flashwave\Desktop\Highlander performence rating system")

PLACEMENT = {
    "gullywash": (9.3, -8464.0, 4761.0), "process": (10.0, -9102.0, 5120.0), "bagel": (9.0, -8192.0, 4608.0),
    "ashville": (8.0, -7322.0, 4101.0), "product": (7.0, -7907.0, 3584.0), "proplant": (9.0, -8192.0, 4608.0),
    "proot": (7.75, -7054.0, 3968.0), "cascade": (8.25, -7512.0, 4226.0), "swiftwater": (8.0, -4381.0, 2726.0),
    "vigil": (7.5, -5802.0, 4940.0), "upward": (5.5, -4956.0, 2216.0), "steel": (8.0, -6740.0, 3196.0),
}
PREFIX = {"product": "koth_product", "proot": "koth_proot", "ashville": "koth_ashville", "vigil": "pl_vigil",
          "upward": "pl_upward", "swiftwater": "pl_swiftwater", "steel": "cp_steel", "bagel": "koth_bagel",
          "cascade": "koth_cascade", "gullywash": "cp_gullywash", "process": "cp_process"}


def overview_to_game(base, p):
    scale, x, y = PLACEMENT[base]
    size = 1024.0 * scale
    cx, cy = x + 910.0 * scale, y - 512.0 * scale
    min_x, max_y = cx - size / 2, cy + size / 2
    return [round(min_x + p[0] / 1440 * size), round(max_y - p[1] / 1440 * size)]


def affine(pairs):
    """Least-squares affine from [(src, dst)], src and dst (x, y)."""
    # Solve dst = A @ [x, y, 1] for each output coordinate.
    import itertools
    n = len(pairs)
    def solve(vals):
        # normal equations for 3 unknowns
        s = [[0.0] * 3 for _ in range(3)]
        r = [0.0] * 3
        for (sx, sy), v in zip([p[0] for p in pairs], vals):
            row = [sx, sy, 1.0]
            for i in range(3):
                r[i] += row[i] * v
                for j in range(3):
                    s[i][j] += row[i] * row[j]
        # Gaussian elimination
        m = [s[i] + [r[i]] for i in range(3)]
        for i in range(3):
            piv = max(range(i, 3), key=lambda k: abs(m[k][i]))
            m[i], m[piv] = m[piv], m[i]
            for k in range(3):
                if k != i:
                    f = m[k][i] / m[i][i]
                    m[k] = [a - f * b for a, b in zip(m[k], m[i])]
        return [m[i][3] / m[i][i] for i in range(3)]
    ax = solve([p[1][0] for p in pairs])
    ay = solve([p[1][1] for p in pairs])
    def f(p):
        return (ax[0] * p[0] + ax[1] * p[1] + ax[2], ay[0] * p[0] + ay[1] * p[1] + ay[2])
    err = max(((f(s)[0] - d[0]) ** 2 + (f(s)[1] - d[1]) ** 2) ** 0.5 for s, d in pairs)
    return f, err


def rect(x0, y0, x1, y1):
    return [(x0, y0), (x1, y0), (x1, y1), (x0, y1)]


MAPS = {}

# ---- koth_product_rcx (Java59 v3.1) --------------------------------------
# The callout image lies on its side: spawn at the left, the point at the
# right; the app's overview has RED's spawn at the top and the point in the
# middle. Landmarks: the point, the rock beside it, the far rock, spawn.
MAPS["product"] = dict(
    pairs=[((1505, 595), (720, 720)), ((1370, 595), (722, 655)), ((1630, 600), (722, 785)),
           ((230, 590), (720, 65)), ((1360, 865), (582, 645)), ((1650, 300), (872, 787))],
    shared=["Point", "Connector", "Dog bread"],
    mirror="y",  # BLU repeats RED across the viaduct
    zones=[
        ("Point", rect(1420, 500, 1590, 700)),
        ("Connector", rect(1450, 790, 1560, 915)),
        ("Rock", rect(1325, 560, 1415, 635)),
        ("Perch", rect(1335, 325, 1410, 395)),
        ("Small rock", rect(1305, 390, 1350, 430)),
        ("Dog bread", rect(1410, 375, 1595, 425)),
        ("Bridge", rect(1053, 295, 1105, 460)),
        ("China", rect(1018, 460, 1160, 550)),
        ("Japan", rect(1155, 645, 1190, 790)),
        ("House", rect(1060, 550, 1190, 695)),
        ("Roof", rect(1370, 920, 1400, 995)),
        ("Stairs", rect(1185, 885, 1280, 960)),
        ("Concrete", [(1245, 780), (1300, 745), (1440, 745), (1440, 910), (1370, 925), (1280, 925), (1245, 880)]),
        ("Left", [(1105, 300), (1340, 300), (1330, 420), (1410, 440), (1410, 500), (1190, 550), (1165, 490), (1105, 460)]),
        ("Valley", [(1190, 550), (1410, 500), (1410, 745), (1285, 745), (1190, 650)]),
        ("Hill", [(1000, 180), (1120, 200), (1340, 240), (1340, 300), (1105, 300), (1000, 300)]),
        ("Boards", rect(683, 505, 735, 600)),
        ("Right", rect(730, 750, 880, 865)),
        ("Base left", rect(655, 225, 800, 340)),
        ("Backyard", [(800, 245), (905, 245), (1000, 300), (1053, 300), (1053, 460), (1018, 460), (1015, 665), (855, 605), (795, 600), (795, 375), (870, 330)]),
        ("Main", [(855, 605), (1015, 665), (1060, 695), (1150, 700), (1150, 760), (1100, 800), (880, 860), (880, 745), (855, 745)]),
        ("Grass", [(805, 870), (880, 865), (1000, 885), (1120, 880), (1150, 905), (1250, 925), (1255, 1000), (1240, 1060), (1100, 1075), (900, 1060), (805, 1000)]),
        ("Base", [(355, 300), (455, 270), (610, 285), (655, 340), (700, 375), (795, 375), (795, 500), (683, 505), (683, 605), (705, 740), (605, 745), (605, 835), (455, 835), (455, 745), (355, 745)]),
        ("Spawn", rect(95, 445, 355, 695)),
    ],
    source="Callouts by Java59 (koth_product_rcx v3.1), traced onto the more.tf overview.",
)


# ---- koth_proot (Java59 v2.2; its corner says koth_product_rcx: the
# template's, not the map's) -------------------------------------------------
MAPS["proot"] = dict(
    pairs=[((1490, 520), (720, 720)), ((1315, 510), (725, 638)), ((1665, 510), (725, 805)),
           ((190, 415), (805, 85)), ((1130, 960), (518, 554)), ((1100, 175), (879, 541))],
    shared=["Point", "Half", "Barn"],
    mirror="y",
    zones=[
        ("Point", rect(1410, 380, 1570, 690)),
        ("Half", rect(1425, 335, 1555, 380)),
        ("Barn", rect(1425, 715, 1555, 805)),
        ("Connector", rect(1350, 715, 1425, 805)),
        ("Chicken", rect(1260, 450, 1370, 575)),
        ("Left", [(1220, 330), (1370, 330), (1370, 450), (1260, 450), (1255, 395)]),
        ("Plank", rect(1130, 290, 1225, 370)),
        ("Roof", rect(1040, 450, 1110, 575)),
        ("Valley", [(1035, 345), (1180, 370), (1260, 450), (1260, 575), (1110, 690), (1035, 580)]),
        ("Main", [(1110, 690), (1260, 575), (1340, 575), (1345, 715), (1115, 715)]),
        ("House", rect(895, 265, 1035, 545)),
        ("Batts", rect(905, 545, 1035, 715)),
        ("Stairs", rect(1085, 715, 1200, 885)),
        ("Shack", rect(1235, 885, 1315, 945)),
        ("Platform", rect(1345, 975, 1420, 1105)),
        ("Flank", [(770, 720), (1085, 720), (1085, 885), (1200, 885), (1340, 800), (1490, 805), (1490, 975), (1420, 975), (1420, 1100), (1110, 1085), (1000, 1000), (870, 955), (770, 945)]),
        ("Hill", [(1005, 140), (1175, 140), (1180, 205), (1200, 290), (1130, 340), (1005, 345)]),
        ("Backyard", [(740, 140), (1005, 140), (1005, 345), (1035, 345), (1035, 450), (895, 545), (895, 715), (650, 715), (650, 460), (740, 330)]),
        ("Short", rect(490, 145, 745, 330)),
        ("Mid", rect(535, 330, 650, 615)),
        ("Long", [(485, 620), (710, 620), (720, 715), (860, 720), (860, 805), (630, 805), (485, 720)]),
        ("Base", [(335, 120), (485, 120), (485, 210), (530, 300), (535, 615), (485, 625), (485, 715), (235, 710), (235, 525), (335, 525)]),
        ("Spawn", rect(50, 170, 335, 520)),
    ],
    source="Callouts by Java59 (koth_proot v2.2), traced onto the more.tf overview.",
)


# ---- koth_ashville_rc2d (Java59 v4.0): BLU's half and the point ----------
# Ashville's halves are the same half turned 180 degrees about the point,
# not mirrored: RED's spawn is bottom right where BLU's is top left.
MAPS["ashville"] = dict(
    pairs=[((1380, 520), (720, 720)), ((390, 650), (648, 85)), ((1140, 805), (538, 566)), ((1220, 275), (877, 618))],
    shared=["Point", "Mid"],
    mirror="rot",
    sides=("BLU", "RED"),
    zones=[
        ("Point", rect(1330, 462, 1432, 578)),
        ("Toxic", rect(1195, 320, 1270, 420)),
        ("Roof", rect(1330, 285, 1425, 410)),
        ("Ramp", [(1110, 230), (1330, 230), (1330, 285), (1270, 320), (1175, 320), (1175, 330), (1110, 340)]),
        ("Fans", rect(1190, 530, 1230, 700)),
        ("Cozy", rect(1230, 610, 1335, 745)),
        ("Stairs", rect(1120, 600, 1170, 695)),
        ("Battlements", [(1065, 700), (1230, 700), (1280, 760), (1335, 760), (1335, 880), (1050, 880)]),
        ("Dirt", [(800, 700), (1065, 700), (1050, 880), (1020, 880), (1020, 840), (890, 840), (890, 915), (800, 915)]),
        ("Main", rect(725, 520, 860, 640)),
        ("Right lobby", [(650, 590), (1170, 560), (1170, 695), (1065, 700), (690, 700), (690, 610)]),
        ("Left lobby", [(650, 410), (910, 410), (915, 230), (1075, 230), (1105, 300), (1105, 340), (1175, 330), (1175, 505), (650, 510)]),
        ("Base", [(485, 380), (650, 380), (650, 690), (790, 690), (790, 915), (655, 915), (655, 880), (510, 880), (485, 850)]),
        ("Spawn", rect(300, 470, 475, 840)),
        ("Mid", [(1195, 320), (1330, 320), (1430, 285), (1565, 285), (1565, 800), (1430, 800), (1430, 625), (1335, 625), (1335, 745), (1230, 745), (1230, 610), (1195, 535)]),
    ],
    source="Callouts by Java59 (koth_ashville_rc2d v4.0), traced onto the more.tf overview.",
)


# ---- pl_vigil (Java59 v5.0): one way, drawn whole ------------------------
MAPS["vigil"] = dict(
    pairs=[((530, 635), (135, 760)), ((530, 770), (135, 930)), ((1230, 255), (985, 295)),
           ((833, 475), (510, 580)), ((1125, 470), (860, 560))],
    zones=[
        # Last (D)
        ("Red room", rect(1135, 195, 1235, 230)),
        ("D main", rect(1195, 150, 1260, 195)),
        ("Roof (D)", rect(1260, 125, 1310, 160)),
        ("Big door", rect(1310, 150, 1365, 200)),
        ("Dropdown", rect(1260, 90, 1320, 125)),
        ("Map", rect(1205, 280, 1270, 370)),
        ("Last", [(1235, 200), (1335, 200), (1335, 330), (1270, 320), (1240, 300)]),
        ("Batts", [(1270, 310), (1420, 310), (1420, 370), (1300, 380)]),
        ("Coffee", rect(1340, 225, 1420, 305)),
        ("One", rect(1420, 215, 1465, 400)),
        ("RED spawn (C+D)", [(1225, 370), (1385, 370), (1380, 450), (1260, 450)]),
        ("Lift", rect(1105, 240, 1200, 270)),
        ("Observatory", rect(1110, 270, 1200, 360)),
        ("D-C connector", rect(1175, 365, 1225, 410)),
        ("Mop", rect(990, 155, 1100, 265)),
        ("Cliff (D)", [(1030, 165), (1130, 110), (1260, 40), (1380, 50), (1420, 110), (1420, 210), (1365, 210), (1340, 140), (1250, 125), (1140, 140)]),
        # C
        ("C", rect(1015, 320, 1100, 375)),
        ("Ladder", rect(975, 360, 1015, 450)),
        ("C main", [(1015, 375), (1075, 375), (1075, 505), (1015, 530), (985, 500), (985, 450)]),
        ("Concrete", rect(1075, 375, 1175, 435)),
        ("RED spawn (A+B)", rect(1075, 435, 1165, 505)),
        ("BLU spawn (C)", rect(790, 435, 875, 515)),
        ("Cliffside (C)", [(870, 285), (1005, 240), (1060, 270), (1015, 320), (975, 360), (960, 450), (915, 500), (875, 500), (860, 420)]),
        # B
        ("Stairs", rect(1060, 505, 1120, 535)),
        ("Lockers", rect(880, 535, 945, 630)),
        ("BBQ", [(945, 535), (1200, 505), (1200, 630), (945, 630)]),
        ("Far", [(1165, 455), (1250, 470), (1340, 560), (1335, 600), (1265, 580), (1235, 590), (1200, 510)]),
        ("Platform", rect(1235, 600, 1265, 700)),
        ("B", [(1265, 580), (1335, 600), (1330, 780), (1290, 785), (1265, 700)]),
        ("Crane", rect(1330, 600, 1365, 780)),
        ("Hut (B)", rect(1210, 705, 1245, 740)),
        ("Roof (B)", rect(1245, 760, 1285, 785)),
        ("Side tunnel", rect(1040, 630, 1110, 665)),
        ("Hill", [(1040, 665), (1200, 630), (1240, 590), (1245, 760), (1210, 790), (1125, 820), (1115, 800), (1065, 780), (1040, 700)]),
        # A
        ("A", rect(1115, 820, 1195, 905)),
        ("Back fence", rect(1195, 830, 1265, 900)),
        ("Crates", rect(1185, 910, 1270, 960)),
        ("Pool", rect(1265, 790, 1380, 905)),
        ("Jenkins", [(1270, 905), (1310, 905), (1305, 1000), (1225, 1050), (1200, 1015), (1260, 960)]),
        ("Combo hold", rect(1000, 960, 1125, 1000)),
        ("Rocks", rect(1070, 800, 1115, 880)),
        ("Truck", rect(860, 855, 905, 885)),
        ("Hut (A)", rect(880, 770, 910, 810)),
        ("A main", [(910, 875), (1065, 860), (1065, 925), (965, 925)]),
        ("A hill", [(905, 770), (1065, 780), (1065, 860), (955, 860)]),
        ("House", [(810, 885), (965, 885), (965, 975), (905, 975), (905, 1045), (860, 1045), (810, 975)]),
        ("Cliffside (A)", [(905, 975), (1000, 1000), (1200, 1000), (1250, 1010), (1220, 1060), (1100, 1090), (950, 1080), (905, 1045)]),
        ("Elbow", [(960, 720), (1010, 700), (1010, 820), (970, 840), (955, 760)]),
        ("Banana", [(850, 645), (1000, 645), (1010, 700), (975, 740), (930, 700), (850, 700)]),
        ("BLU spawn", [(530, 635), (850, 635), (850, 700), (885, 700), (885, 770), (770, 770), (770, 840), (690, 840), (690, 770), (530, 770)]),
    ],
    source="Callouts by Java59 (pl_vigil v5.0), traced onto the more.tf overview.",
)


# ---- pl_upward (Java59 v6.0): one way, drawn whole ------------------------
# The callout image is stretched differently along each axis; the affine fit
# takes that in. The "layers" insets (upper floors, tunnels) are left out:
# seen from above they sit on top of the ground they cover.
UPWARD_ZONES = [
    # D, last
    ("D crates", rect(1010, 560, 1075, 590)),
    ("Back wall", rect(1150, 450, 1215, 560)),
    ("Last", [(955, 440), (1215, 410), (1225, 580), (1135, 650), (975, 650), (950, 560)]),
    ("RED spawn (last)", rect(985, 255, 1140, 370)),
    ("Lower spawn exit", rect(990, 370, 1150, 410)),
    ("Toxic", rect(1150, 355, 1215, 410)),
    ("Apartments", rect(1205, 255, 1290, 330)),
    ("Side tunnel", rect(1225, 450, 1310, 550)),
    ("D stairs", rect(985, 715, 1045, 745)),
    ("Minecart", rect(1085, 730, 1130, 745)),
    ("Lobby", [(880, 665), (1130, 665), (1130, 745), (900, 745)]),
    ("Far back", [(755, 560), (950, 560), (950, 660), (880, 665), (800, 660), (750, 600)]),
    # C
    ("Spiral", rect(870, 415, 950, 555)),
    ("Elbow", rect(815, 370, 975, 415)),
    ("C platform", rect(865, 315, 975, 345)),
    ("C hut", rect(640, 340, 700, 420)),
    ("Rollercoaster", rect(690, 110, 780, 210)),
    ("C catwalk", rect(865, 205, 920, 255)),
    ("C main", [(780, 190), (900, 130), (975, 255), (985, 300), (870, 300), (860, 270)]),
    ("C", [(690, 210), (780, 190), (860, 270), (860, 365), (815, 370), (750, 560), (700, 540), (690, 400)]),
    # B
    ("B house", rect(1035, 140, 1145, 245)),
    ("Sewers", rect(1040, 105, 1140, 140)),
    ("Ramp", rect(1035, 65, 1145, 100)),
    ("B hut", rect(1345, 60, 1430, 150)),
    ("Behind shack", [(1340, 20), (1460, 30), (1480, 90), (1430, 150), (1430, 60)]),
    ("Trench", [(820, 20), (1340, 20), (1340, 60), (1150, 65), (1035, 65), (900, 70), (820, 110)]),
    ("B platform", rect(1145, 180, 1205, 280)),
    ("B main", rect(1290, 255, 1380, 380)),
    ("B", [(1145, 150), (1345, 150), (1480, 160), (1500, 300), (1440, 330), (1290, 330), (1290, 255), (1205, 255), (1205, 180)]),
    ("Rocks", [(1330, 380), (1480, 360), (1520, 460), (1420, 480), (1330, 450)]),
    ("Cliff", [(1480, 160), (1545, 160), (1560, 420), (1510, 420), (1500, 300)]),
    ("Hill", [(1310, 470), (1520, 460), (1560, 600), (1530, 650), (1400, 650), (1310, 560)]),
    # A
    ("Sandbox", rect(1290, 610, 1395, 700)),
    ("A cliff", rect(1140, 630, 1250, 700)),
    ("Slope", [(1395, 650), (1530, 650), (1560, 770), (1500, 800), (1400, 770), (1360, 700)]),
    ("A", [(1140, 700), (1280, 700), (1360, 760), (1340, 800), (1150, 810)]),
    ("Playground", rect(1145, 815, 1225, 905)),
    ("Behind playground", rect(1260, 835, 1370, 905)),
    ("A secret", rect(1370, 830, 1460, 910)),
    ("Big ammo", [(1150, 905), (1400, 905), (1470, 960), (1420, 1030), (1150, 1020)]),
    ("A stairs", rect(970, 760, 1070, 830)),
    ("Main trench", rect(965, 830, 1140, 900)),
    ("Red rocks", [(920, 900), (1140, 900), (1150, 1020), (1100, 1080), (960, 1080), (910, 1010)]),
    ("Blue rocks", [(700, 690), (800, 680), (915, 760), (880, 790), (740, 770)]),
    ("Tracks", [(570, 640), (750, 620), (880, 700), (965, 760), (965, 900), (910, 1000), (810, 960), (810, 900), (595, 900), (575, 760)]),
    ("Behind roof", [(600, 955), (800, 955), (820, 1030), (800, 1090), (600, 1090)]),
    ("Right spawn", rect(595, 905, 805, 950)),
    ("Left spawn", rect(570, 600, 740, 640)),
    ("Main spawn", rect(475, 700, 570, 950)),
    ("BLU spawn", rect(345, 665, 470, 950)),
]
MAPS["upward"] = dict(
    pairs=[((1080, 510), (812, 640)), ((1390, 110), (1175, 190)), ((1040, 960), (820, 1190)), ((820, 760), (560, 990)), ((1200, 25), (900, 40))],
    zones=UPWARD_ZONES,
    source="Callouts by Java59 (pl_upward v6.0), traced onto the more.tf overview.",
)


# ---- pl_swiftwater (Java59 v5.0): one way, drawn whole --------------------
MAPS["swiftwater"] = dict(
    pairs=[((997, 218), (728, 288)), ((835, 218), (520, 285)), ((686, 846), (328, 1095)),
           ((890, 955), (598, 1233)), ((1170, 745), (950, 955)), ((1335, 445), (1160, 580))],
    zones=[
        # E
        ("Secret", rect(830, 12, 935, 70)),
        ("Barn", rect(750, 75, 880, 125)),
        ("Map room", rect(885, 75, 965, 145)),
        ("RPC", rect(1060, 70, 1100, 150)),
        ("Box", rect(1045, 150, 1100, 190)),
        ("E", rect(945, 180, 1040, 265)),
        ("Grass", [(965, 50), (1060, 50), (1100, 200), (1100, 265), (1040, 265), (1040, 180), (965, 180), (945, 145)]),
        ("Lockers", rect(880, 145, 945, 295)),
        ("Empty", rect(965, 280, 1100, 310)),
        # D
        ("Gate", rect(820, 130, 880, 160)),
        ("House (D)", rect(680, 150, 760, 235)),
        ("Ballpit", rect(800, 180, 870, 255)),
        ("Catwalk", rect(870, 180, 895, 255)),
        ("Behind (D)", [(650, 75), (750, 75), (750, 150), (680, 155), (650, 190)]),
        ("Hold (D)", [(650, 190), (680, 155), (680, 235), (800, 255), (880, 255), (900, 300), (880, 310), (650, 310)]),
        ("Rocks", rect(785, 350, 815, 440)),
        ("Shack", rect(878, 378, 905, 405)),
        ("Slope", [(815, 320), (905, 320), (905, 460), (850, 470), (820, 440)]),
        ("Porch", rect(650, 435, 690, 480)),
        ("Garage", rect(685, 440, 790, 560)),
        ("View", rect(790, 515, 855, 560)),
        ("Yard", [(650, 310), (815, 310), (830, 350), (830, 440), (760, 470), (690, 470), (650, 440)]),
        # C
        ("Choke", rect(900, 455, 935, 480)),
        ("Balcony", rect(930, 315, 1035, 390)),
        ("Einstein", rect(1035, 315, 1100, 390)),
        ("Long", rect(1100, 310, 1140, 440)),
        ("Tanks", rect(855, 485, 935, 600)),
        ("Windows", rect(935, 525, 1055, 560)),
        ("Patio", rect(935, 560, 1025, 605)),
        ("C", [(935, 390), (1060, 390), (1060, 480), (1100, 480), (1100, 520), (935, 520)]),
        # B
        ("Attic", rect(1060, 520, 1120, 560)),
        ("Shutter", rect(1025, 600, 1100, 665)),
        ("RED spawn (B)", rect(1140, 260, 1230, 345)),
        ("Mansion", rect(1285, 400, 1385, 500)),
        ("Tires", rect(1195, 480, 1290, 560)),
        ("Dirt", rect(1290, 470, 1365, 570)),
        ("Banana", rect(1260, 570, 1360, 705)),
        ("B", [(1100, 390), (1200, 385), (1240, 470), (1260, 560), (1290, 660), (1100, 665), (1100, 560), (1140, 520), (1140, 440)]),
        ("Arm", rect(1030, 760, 1090, 830)),
        ("Pallet", rect(1095, 785, 1280, 830)),
        ("Tunnel", [(1035, 670), (1260, 670), (1285, 700), (1280, 830), (1030, 830)]),
        # A
        ("Behind (A)", rect(785, 595, 905, 660)),
        ("Bar", rect(730, 650, 810, 710)),
        ("Stairs (A)", rect(815, 655, 845, 700)),
        ("Boxes", rect(688, 760, 735, 805)),
        ("Hold (A)", rect(765, 755, 880, 810)),
        ("A", [(905, 640), (1000, 650), (1010, 700), (1030, 740), (1000, 830), (940, 850), (900, 760)]),
        ("Roof", rect(955, 840, 1025, 890)),
        ("Stream", [(790, 845), (950, 835), (955, 885), (800, 890)]),
        ("BLU spawn", [(855, 930), (935, 930), (1000, 985), (1000, 1030), (935, 1030), (935, 1080), (865, 1080), (855, 1030)]),
        ("Left", [(700, 870), (790, 845), (800, 890), (855, 930), (855, 1030), (745, 1030), (700, 960)]),
        ("Right", [(940, 890), (1040, 880), (1040, 1010), (1000, 1030), (935, 985)]),
    ],
    source="Callouts by Java59 (pl_swiftwater v5.0), traced onto the more.tf overview.",
)


def build(base, spec):
    f, err = affine(spec["pairs"])
    # The traced side's team, and how the other side is made from it.
    first, second = spec.get("sides", ("RED", "BLU"))
    other = {
        "y": lambda x, y: (x, 1440 - y),
        "x": lambda x, y: (1440 - x, y),
        "rot": lambda x, y: (1440 - x, 1440 - y),
    }.get(spec.get("mirror"))
    zones = []
    # In the order given: where zones overlap, the earlier one wins.
    for name, pts in spec["zones"]:
        ov = [f(p) for p in pts]
        if other and name not in spec.get("shared", []):
            zones.append({"name": f"{first} {name}", "points": [overview_to_game(base, p) for p in ov]})
            zones.append({"name": f"{second} {name}", "points": [overview_to_game(base, other(*p)) for p in ov]})
        else:
            zones.append({"name": name, "points": [overview_to_game(base, p) for p in ov]})
    return zones, err


def inside(p, poly):
    x, y = p
    c = False
    n = len(poly)
    for i in range(n):
        x1, y1 = poly[i]
        x2, y2 = poly[(i + 1) % n]
        if (y1 > y) != (y2 > y) and x < (x2 - x1) * (y - y1) / (y2 - y1) + x1:
            c = not c
    return c


def check(db, base, zones):
    rows = db.execute(
        "SELECT k.kx, k.ky, k.vx, k.vy, k.victim_team FROM kill_event k JOIN match m ON m.log_id = k.log_id WHERE m.map LIKE ? AND k.kx IS NOT NULL",
        (PREFIX[base] + "%",),
    ).fetchall()
    cnt = Counter()
    sides = Counter()
    tot = 0
    for kx, ky, vx, vy, vteam in rows:
        for p in [(kx, ky), (vx, vy)]:
            tot += 1
            z = next((z["name"] for z in zones if inside(p, z["points"])), None)
            cnt[z] += 1
        # Where victims fell, by the side a zone is named for.
        z = next((z["name"] for z in zones if inside((vx, vy), z["points"])), None)
        if z and z.split(" ")[0] in ("RED", "BLU") and z.endswith(("Spawn", "Base")):
            sides[(z.split(" ")[0], vteam)] += 1
    return tot, cnt, sides


if __name__ == "__main__":
    db = sqlite3.connect(f"file:{sys.argv[1]}?mode=ro", uri=True)
    todo = sys.argv[2:] or list(MAPS)
    for base in todo:
        spec = MAPS[base]
        zones, err = build(base, spec)
        tot, cnt, sides = check(db, base, zones)
        inzone = tot - cnt[None]
        print(f"{base}: {len(zones)} zones, landmark fit within {err:.0f} px; {tot} positions, {100 * inzone / max(1, tot):.0f}% in a zone")
        print("   " + ", ".join(f"{k} {v}" for k, v in cnt.most_common() if k))
        if sides:
            print("   victims in spawn/base zones, (zone side, victim team):", dict(sides))
        f = {"map": base, "draft": True, "source": spec["source"] + " Not yet checked in game.", "zones": zones, "names": []}
        with open(f"callouts/{base}.json", "w", encoding="utf-8", newline="\n") as fh:
            json.dump(f, fh, indent=1, ensure_ascii=False)
            fh.write("\n")
