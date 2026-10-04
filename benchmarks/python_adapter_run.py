"""Run identical supported public calls with one selected Python engine."""
import importlib
import sys
engine = importlib.import_module(sys.argv[1])
workload, rows, source, target = sys.argv[2], int(sys.argv[3]), sys.argv[4], sys.argv[5]
if workload == "create":
    workbook = engine.Workbook()
    sheet = workbook.active
    for row in range(rows):
        sheet.append([row * 10 + column for column in range(10)])
elif workload == "edit":
    workbook = engine.load_workbook(source)
    workbook["Sheet"]["A1"].value = -1
else:
    raise ValueError("Unknown adapter workload")
workbook.save(target)
workbook.close()
print("Saved")
