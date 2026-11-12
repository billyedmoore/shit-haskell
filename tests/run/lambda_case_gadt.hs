{-# LANGUAGE GADTs, ConstraintKinds, KindSignatures, RankNTypes, LambdaCase, DataKinds, TypeApplications, TypeFamilies, FlexibleInstances #-}
-- A lambda-case checked against its signature: its alternatives see the argument type, so a GADT
-- and data-family pattern with a pattern guard giving constraints work (Cabal's runRuleDynDepsCmd).
import Data.Kind (Constraint, Type)
data Scope = User | System
data Dict (c :: Constraint) where
  Dict :: c => Dict c
newtype Ptr a = Ptr a
deRef :: Ptr a -> a
deRef (Ptr a) = a
data family Static (scope :: Scope) :: Type -> Type
newtype instance Static 'User fnTy = UserStatic (Ptr fnTy)
newtype ScopedArgument (scope :: Scope) arg = ScopedArgument {getArg :: arg}
class Enc a where enc :: a -> String
instance Enc Int where enc = show
instance Enc arg => Enc (ScopedArgument 'User arg) where enc (ScopedArgument a) = "U" ++ enc a
data Cmds (scope :: Scope) where
  Static0 :: Cmds scope
  Dyn :: forall depsRes scope. { insts :: Static scope (Dict (Enc depsRes, Show depsRes)), action :: IO depsRes } -> Cmds scope
run :: Cmds 'User -> Maybe (IO String)
run = \case
  Static0 -> Nothing
  Dyn { insts = UserStatic ptr, action = act }
    | Dict <- deRef ptr -> Just $ do
        r <- act
        return (enc (ScopedArgument @'User r) ++ show r)
main = maybe (return ()) (>>= putStrLn) (run (Dyn (UserStatic (Ptr Dict)) (return (5 :: Int))))
