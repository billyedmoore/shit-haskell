{-# LANGUAGE FunctionalDependencies #-}

class EmptySide a b | -> a, b ->
class Repeated a b | a a -> b b, a -> b

main :: IO ()
main = pure ()
