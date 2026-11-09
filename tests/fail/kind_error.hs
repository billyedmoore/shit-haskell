f :: Maybe -> Int
f _ = 0

main :: IO ()
main = print (f undefined)
