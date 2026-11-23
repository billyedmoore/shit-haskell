data T = A { item :: Int } | B { item :: Int }
main :: IO ()
main = print (item (B 3))
