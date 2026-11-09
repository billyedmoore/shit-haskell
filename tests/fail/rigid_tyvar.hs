f :: a -> a
f _ = True

main :: IO ()
main = print (f 'c')
