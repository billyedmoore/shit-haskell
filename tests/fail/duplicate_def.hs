f :: Int -> Int
f 0 = 1
g :: Int
g = 2
f n = n

main :: IO ()
main = print (f g)
