{-# LANGUAGE TypeFamilies, NoDatatypeContexts #-}

data family F a
data instance Eq a => F [a] = Mk a

main :: IO ()
main = pure ()
