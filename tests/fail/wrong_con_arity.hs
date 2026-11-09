data P = P Int Int

f :: P -> Int
f (P a) = a

main :: IO ()
main = print (f (P 1 2))
