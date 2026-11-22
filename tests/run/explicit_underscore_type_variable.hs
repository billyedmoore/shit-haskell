same :: forall _a. _a -> _a
same x = (x :: _a)

main = print (same True)
