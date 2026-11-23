f :: forall . a -> a
f x = x

main :: IO ()
main = print (f True)
