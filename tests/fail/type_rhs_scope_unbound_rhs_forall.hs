{-# LANGUAGE RankNTypes #-}
type T a = forall b. a -> missing -> b
main :: IO ()
main = pure ()
