{-# LANGUAGE RankNTypes #-}
data T a = T (forall b. b -> a)
main :: IO ()
main = pure ()
