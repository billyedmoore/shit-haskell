{-# LANGUAGE StandaloneKindSignatures,TypeFamilies,PolyKinds,TypeOperators,TypeAbstractions #-}
import Data.Kind (Type)
data T _ _ = T Int
main :: IO ()
main = case (T 4 :: T Bool Char) of T n -> print n
