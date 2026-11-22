{-# LANGUAGE NoBinaryLiterals #-}

choose 0b0 = b0
choose _ x = x

main = print (choose 0 True)
