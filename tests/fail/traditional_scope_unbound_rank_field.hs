{-# LANGUAGE RankNTypes #-}
data T a = T (forall b. b -> missing)
main :: IO ()
main = pure ()
