{-# LANGUAGE FunctionalDependencies, PolyKinds #-}

class Pair (a :: k) b | k -> b

main :: IO ()
main = pure ()
