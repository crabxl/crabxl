"""Measure identical public formula translation calls through one engine."""
import hashlib
import importlib
import sys
Translator = importlib.import_module(sys.argv[1] + ".formula.translate").Translator
count, mode = int(sys.argv[2]), sys.argv[3]
expression = '=SUM(A1:B2)+\'A1\'!$C3+T1[A1]+LOG10(D4)+"A1"'
translator = Translator(expression, "A1")
length = 0
for _ in range(count):
    current = Translator(expression, "A1") if mode == "construct" else translator
    value = current.translate_formula("B2")
    length += len(value)
print(count, length, hashlib.sha256(value.encode()).hexdigest())
