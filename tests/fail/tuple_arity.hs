f :: (Int, Int) -> Int
f (a, b, c) = a

main :: IO ()
main = print (f (1, 2))
