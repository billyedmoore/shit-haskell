{-# LANGUAGE MultiParamTypeClasses #-}
class Broken a a where method :: a -> a
main :: IO ()
main = pure ()
