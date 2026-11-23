{-# LANGUAGE FunctionalDependencies #-}
class Good a b | a a -> b where method :: a -> b
main :: IO ()
main = pure ()
