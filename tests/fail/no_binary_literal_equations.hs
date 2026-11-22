{-# LANGUAGE NoBinaryLiterals #-}

choose 0b0 = True
choose _ = False

main = print (choose 0)
