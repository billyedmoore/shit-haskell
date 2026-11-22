{-# LANGUAGE TypeFamilies, DatatypeContexts #-}

data family F a
data instance Eq a => F [a] = Mk a

main :: IO ()
main = case Mk (4 :: Int) of Mk x -> print x
