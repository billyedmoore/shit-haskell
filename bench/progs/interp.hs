-- An interpreter for a small imperative language, run in a State monad over a Map of
-- variables: monadic code, a sum type and dictionary-passing through mtl.
import Control.Monad.State.Strict
import Control.Monad (when)
import qualified Data.Map.Strict as M

data Expr = Lit Int | Var String | Add Expr Expr | Sub Expr Expr | Mul Expr Expr | Lt Expr Expr
data Stmt = Assign String Expr | While Expr [Stmt] | If Expr [Stmt] [Stmt]

eval :: Expr -> State (M.Map String Int) Int
eval (Lit n) = pure n
eval (Var v) = gets (M.findWithDefault 0 v)
eval (Add a b) = (+) <$> eval a <*> eval b
eval (Sub a b) = (-) <$> eval a <*> eval b
eval (Mul a b) = (*) <$> eval a <*> eval b
eval (Lt a b) = (\x y -> fromEnum (x < y)) <$> eval a <*> eval b

exec :: Stmt -> State (M.Map String Int) ()
exec (Assign v e) = eval e >>= \x -> modify' (M.insert v x)
exec w@(While c body) = do
  t <- eval c
  when (t /= 0) (mapM_ exec body >> exec w)
exec (If c t e) = eval c >>= \x -> mapM_ exec (if x /= 0 then t else e)

program :: [Stmt]
program =
  [ Assign "i" (Lit 0), Assign "s" (Lit 0)
  , While (Lt (Var "i") (Lit 200000))
      [ If (Lt (Sub (Var "i") (Mul (Lit 3) (Var "q"))) (Lit 1))
          [Assign "s" (Add (Var "s") (Var "i")), Assign "q" (Add (Var "q") (Lit 1))]
          [Assign "s" (Sub (Var "s") (Lit 1))]
      , Assign "i" (Add (Var "i") (Lit 1)) ] ]

main :: IO ()
main = print (M.toList (execState (mapM_ exec program) M.empty))
