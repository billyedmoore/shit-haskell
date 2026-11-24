{-# LANGUAGE StandaloneKindSignatures,TypeFamilies,PolyKinds,TypeOperators,TypeAbstractions #-}
import Data.Kind (Type)
data T (_ :: Type -> Type) = T Int
main :: IO ()
main = case (T 4 :: T Maybe) of T n -> print n
